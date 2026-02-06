# Asteroid Spectral Class Texture Specifications

## Overview

Each spectral class needs a distinctive texture for visual identification. These textures represent the typical surface appearance based on spectroscopic observations and meteorite analysis.

## Texture Specifications

### C-Type (Carbonaceous) ✅ EXISTS
**File:** `generic_c_type_2k.jpg`
**Appearance:** Very dark gray to black, low albedo (~0.03-0.07)
**Features:**
- Dark carbonaceous material
- Minimal reflectivity
- Slightly mottled surface
- Represents ~75% of asteroids

**Real Examples:** Ceres, Pallas, Hygiea, Themis

---

### S-Type (Silicaceous) ✅ EXISTS  
**File:** `generic_s_type_2k.jpg`
**Appearance:** Medium gray, moderate albedo (~0.10-0.22)
**Features:**
- Stony, rocky surface
- Olivine and pyroxene minerals
- More reflective than C-type
- Represents ~17% of asteroids

**Real Examples:** Vesta (technically V-type but similar), Juno, Iris, Flora

---

### M-Type (Metallic) ⚠️ NEEDED
**File:** `generic_m_type_2k.jpg` (TO BE CREATED)
**Appearance:** Bright silvery-gray, high albedo (~0.10-0.30)
**Features:**
- Metallic iron-nickel appearance
- High reflectivity
- Smooth, shiny surface
- Represents ~8% of asteroids

**Real Examples:** Psyche, Lutetia, Kleopatra

**Creation Guide:**
- Base color: Bright gray #C0C0C0 to #E0E0E0
- Add subtle scratches and impact marks
- High specularity/metallic property
- Reference: polished iron meteorites

---

### V-Type (Vestoid/Basaltic) ⚠️ NEEDED
**File:** `generic_v_type_2k.jpg` (TO BE CREATED)
**Appearance:** Reddish-gray basaltic, moderate-high albedo (~0.30-0.40)
**Features:**
- Basaltic rock appearance
- Pyroxene-rich surface
- Slightly reddish tint
- Rare <1% of asteroids

**Real Examples:** 4 Vesta (parent body), Vesta family members

**Creation Guide:**
- Base color: Gray with red tint #B08080 to #A09090
- Rocky, crystalline texture
- Similar to basalt or volcanic rock
- Reference: Vesta surface images from Dawn mission

---

### D-Type (Dark Primitive) ⚠️ NEEDED
**File:** `generic_d_type_2k.jpg` (TO BE CREATED)
**Appearance:** Extremely dark, very low albedo (<0.05)
**Features:**
- Darkest asteroid type
- Organic-rich, primitive material
- Very red spectrum
- Found in outer belt and Jupiter Trojans

**Real Examples:** Hektor, Patroclus, many Jupiter Trojans

**Creation Guide:**
- Base color: Very dark brown/black #2A1810 to #3A2820
- Organic, sooty appearance
- Minimal reflectivity
- Reference: carbonaceous chondrite meteorites but darker

---

### P-Type (Primitive) ⚠️ NEEDED
**File:** `generic_p_type_2k.jpg` (TO BE CREATED)
**Appearance:** Very dark gray-brown, low albedo (~0.02-0.06)
**Features:**
- Similar to D-type but less red
- Primitive, unaltered material
- Very low reflectivity
- Found in outer belt

**Real Examples:** Sylvia, Camilla, Hestia

**Creation Guide:**
- Base color: Dark gray-brown #3A3A35 to #4A4A40
- Between D-type and C-type in appearance
- Slightly less organic-looking than D-type
- Reference: primitive carbonaceous chondrites

---

## Visual Distinction Strategy

To ensure asteroids are visually distinguishable at a glance:

| Type | Albedo | Color Tint | Brightness | Key Feature |
|------|--------|------------|------------|-------------|
| C-Type | Very Low | Neutral gray | Dark | Most common, very dark |
| S-Type | Medium | Neutral gray | Medium | Rocky, middle brightness |
| M-Type | High | Silvery | Bright | Shiny, metallic |
| V-Type | High | Red-gray | Medium-Bright | Reddish basalt |
| D-Type | Extreme Low | Brown-red | Very Dark | Darkest, organic |
| P-Type | Very Low | Gray-brown | Very Dark | Dark but not reddish |

## Texture Creation Methods

### Option 1: AI Generation (Recommended)
Use Stable Diffusion or similar with prompts:
- M-Type: "metallic asteroid surface, iron-nickel metal, shiny silver gray, space, realistic, 4k"
- V-Type: "basaltic asteroid surface, volcanic rock, reddish gray, crystalline, 4k"
- D-Type: "extremely dark asteroid, organic material, coal black surface, sooty, 4k"
- P-Type: "primitive asteroid surface, dark gray-brown, ancient rock, unaltered, 4k"

### Option 2: Photo Manipulation
Base images from:
- Meteorite photos (metal, stone, carbonaceous)
- Rock textures (basalt, iron ore)
- NASA asteroid images (adjust colors)

### Option 3: Procedural Generation
Use texture generation software (Substance Designer, Blender):
- Create noise patterns
- Apply appropriate colors
- Add surface features (craters, scratches)

## Temporary Solution

Until proper textures are created, the system can:
1. Use generic_c_type for D/P-types (all dark)
2. Use generic_s_type for M/V-types (lighter)
3. Rely on procedural variation (metallic property, color tint)

However, this is not ideal for visual distinction.

## Implementation Priority

1. **M-Type** (highest) - 1 asteroid (Psyche), scientifically important
2. **D-Type** (high) - 14 asteroids, distinct appearance
3. **P-Type** (high) - 24 asteroids, common in dataset
4. **V-Type** (medium) - Can use S-type texture temporarily

## File Specifications

All textures should be:
- **Format:** JPG (good compression for space textures)
- **Size:** 2048x1024 (2K, equirectangular projection)
- **File size:** ~1-1.5 MB after compression
- **Quality:** 85-90% JPEG quality
- **Bit depth:** 8-bit RGB

## References

- Bus-DeMeo Taxonomy (2009)
- NASA Dawn mission images (Vesta)
- JPL Small-Body Database
- Meteorite photographs
- Asterank spectral data
