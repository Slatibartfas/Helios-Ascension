# Asteroid appearance

Asteroids use two levels of deterministic detail:

1. `create_asteroid_mesh` generates the large irregular silhouette from a
   name-derived seed.
2. `StandardMaterial::normal_map_texture` loads the shared
   `generic_rock_normal_2k.png` relief map for small craters and fractured
   regolith.
3. `StandardMaterial::metallic_roughness_texture` loads the shared
   `generic_rock_roughness_2k.png` for micro-variation in roughness across
   the surface (the per-class `metallic` value still drives the
   metal-vs-dielectric reading).

The color and PBR response are selected from the asteroid spectral class in
`src/plugins/solar_system.rs`:

| Class | Appearance | PBR intent |
| --- | --- | --- |
| C | charcoal/neutral dark grey | very rough, non-metallic |
| S | muted grey/tan stone | rough, low metallic |
| M | cool grey metal-rock mix | moderately rough, restrained metallic |
| V | basaltic grey | rough, non-metallic |
| D/P | very dark brown-grey primitive material | extremely rough, non-metallic |

A small per-body jitter (`asteroid_albedo_jitter`) multiplies the class tint
by 0.88–1.12 per channel so two same-class asteroids read with slightly
different tints, but every asteroid still falls in the same albedo range.

These are broad visual priors, not claims that every asteroid in a class has
one exact color. The palette is grounded in NASA mission observations: the
rugged dark Bennu and Ryugu surfaces, muted Eros imagery, Dawn's high
bright/dark contrast on Vesta, and current mixed rock/metal interpretations of
Psyche. The implementation avoids saturated red defaults because returned
mission imagery generally shows neutral charcoal, stone-grey, brown-grey, and
localized albedo variation instead.

## Generic-texture routing

Every non-Vesta asteroid (C, S, M, V, D, P, Unknown) shares the neutral
`generic_c_type_2k.jpg` rock map. The per-class color profile above is a
multiplier applied on top of that neutral base. The previous split — C/D/P
through `generic_c_type_2k.jpg` and S/M/V through the warm
`generic_s_type_2k.jpg` — produced a Mars-like surface for S/M/V bodies
and crushed the shadow side into a black silhouette under any
back-lighting, because Bevy's PBR doesn't model surface interreflection
and a tiny emissive floor can't recover from a texture that already
sits in the dark-brown band. Funnelling every class through one neutral
map keeps the rock readable as rock and lets the class multiplier push
the hue into the right zone without fighting the underlying texture.

Vesta is the only asteroid with a dedicated texture (`vesta_4k.png`);
that path bypasses the generic routing entirely.

## Asset note

The binary normal and roughness maps are supplied as 1024×1024 PNGs. The
authoring pipeline lives in `scripts/`:

- `scripts/bake_rock_normal.py` and `scripts/bake_rock_roughness.py` convert
  the OpenEXR maps shipped in
  `G:\Eigene Dateien\Downloads\dark_rock_02_4k.blend\textures\` into Bevy-
  compatible PNGs. The EXR is the canonical source; the PNG is what the
  runtime loads because Bevy 0.18's default asset features do not include
  OpenEXR. Re-run the bakers whenever the EXR is replaced.
- `scripts/generate_asteroid_normal.py` is the procedural fallback used to
  produce a starter normal map when the EXR is unavailable. It is not used
  by the runtime as long as the baked PNG is in place.
- Do not commit NASA mission photographs as texture assets without checking
  their image licensing and attribution requirements.
