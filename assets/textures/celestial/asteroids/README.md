# Asteroid Textures

Textures for asteroid bodies. The game deliberately uses a single shared
rock map for all non-Vesta asteroids rather than per-class PNGs — the
per-class color profile in `asteroid_class_profile` does the hue
differentiation on top of one neutral base, which keeps the dark side
legible under any lighting angle.

## Available Textures

- **Generic rock (used by every C/S/M/V/D/P/Unknown asteroid and by
  dwarf planets / moons that fall through to the generic fallback):**
  `generic_c_type_2k.jpg` — neutral grey rock with cratered relief.
- **`generic_s_type_2k.jpg`** — kept on disk for reference; the generic
  routing no longer points at it because its warm pink-brown splotches
  read as a Mars-like surface and crush the dark side into a
  black silhouette under back-lighting.
- **`vesta_4k.png`** — dedicated basaltic albedo for Vesta. Vesta is the
  only asteroid with a dedicated texture and bypasses the generic
  fallback.

## Spectral Type Mapping

The game uses these textures based on asteroid classification:
- **C-type:** Most common in outer main belt (~75% of asteroids).
  Multiplier `~0.55` over the neutral rock — keeps the rock its
  natural charcoal grey.
- **S-type:** Common in inner main belt (~17% of asteroids). Multiplier
  `~0.78` with a small warm bias — overlays warm grey on top of the
  neutral rock.
- **M-type:** Metal-rich asteroids (procedurally generated). Cool grey
  multiplier + raised `metallic` to suggest metal-silicate mix.
- **V-type:** Basaltic asteroids from Vesta family (uses Vesta
  texture when available; otherwise the generic rock + basaltic brown
  multiplier).

## Normal-map workflow

Asteroid and comet materials use two shared tangent-space relief maps that
ship in this folder:

- `generic_rock_normal_2k.png` — primary relief (craters, fractured
  regolith, faceted rock faces). Loaded as the material's
  `normal_map_texture` for every asteroid and comet. Bevy 0.18 does
  not expose a public `normal_map_strength` knob, so the relief reads
  at the default scale; the map is dense enough to be visible at
  in-game zoom.
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
