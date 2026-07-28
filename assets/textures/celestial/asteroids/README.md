# Asteroid Textures

Textures for different asteroid spectral types based on composition.

## Available Textures

- **C-type (Carbonaceous):** `generic_c_type_2k.jpg` - Dark, carbon-rich asteroids
- **S-type (Silicaceous):** `generic_s_type_2k.jpg` - Stony asteroids
- **Ceres:** `4k_ceres.jpg` - Largest C-type asteroid (dwarf planet)
- **Vesta:** `vesta_4k.png` - Largest V-type asteroid (basaltic)

## Spectral Type Mapping

The game uses these textures based on asteroid classification:
- **C-type:** Most common in outer main belt (~75% of asteroids)
- **S-type:** Common in inner main belt (~17% of asteroids)
- **M-type:** Metal-rich asteroids (procedurally generated)
- **V-type:** Basaltic asteroids from Vesta family (uses Vesta texture)

## Normal-map workflow

Asteroid and comet materials use two shared tangent-space relief maps that
ship in this folder:

- `generic_rock_normal_2k.png` — primary relief (craters, fractured
  regolith, faceted rock faces). Loaded as the material's
  `normal_map_texture` for every asteroid and comet, with
  `normal_map_strength = 1.4` so the relief is visible at in-game zoom.
- `generic_rock_roughness_2k.png` — single-channel roughness in the
  green channel (GLTF convention). Loaded as the material's
  `metallic_roughness_texture`; the per-class `metallic` value still
  drives the metallic reading, this map only adds micro-variation.

The class palette is intentionally subdued and mission-informed: Bennu and
Ryugu motivate charcoal, rugged C-types; Eros motivates muted grey/tan
S-types; Dawn's Vesta imagery motivates basaltic grey with albedo variation;
and Psyche observations motivate mixed grey rock/metal rather than a uniformly
shiny metallic surface. Per-name jitter is deterministic so save/load and new
runs produce the same appearance.
