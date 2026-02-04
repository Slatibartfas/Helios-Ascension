# Texture Download Sources

Due to download restrictions, textures cannot be automatically downloaded. Please download manually from these sources:

## High-Quality 8K Textures (Already Downloaded)

The following 8K textures are already included:
- Sun, Mercury, Venus, Earth, Mars, Jupiter, Saturn, Moon

Source: Solar System Scope (CC BY 4.0)
https://www.solarsystemscope.com/textures/

## Moon Textures (Need Manual Download)

For the moon textures that are currently placeholders, you can download from:

### Option 1: Solar System Scope (Free, CC BY 4.0)
Visit: https://www.solarsystemscope.com/textures/

Download these files (2K resolution):
- Phobos: 2k_phobos.jpg → rename to phobos_2k.jpg (Verified Availability)
- Deimos: 2k_deimos.jpg → rename to deimos_2k.jpg (Verified Availability)

### Option 2: USGS Astrogeology (Scientific, High Accuracy)

**Phobos:**
- **Primary Source (Mars Express SRC):** [Phobos ME SRC Mosaic Global 1024.jpg](https://astrogeology.usgs.gov/ckan/dataset/ca781f2d-0e29-4560-a14e-1b41269c74a9/resource/6c47165c-4094-4ff5-8a21-065deb4319d6/download/phobos_me_src_mosaic_global_1024.jpg)
  - Resolution: 1024x512
  - Description: Global mosaic from Mars Express Super Resolution Channel.
  - Verification: **Verified Link** (Direct Download)

- **Alternative (Viking):** [Phobos Viking Global Mosaic](https://astrogeology.usgs.gov/search/map/phobos_viking_global_mosaic_5m)
  - Higher resolution TIF available.

**Deimos:**
- **Search Query:** [USGS Deimos Search](https://astrogeology.usgs.gov/search/results?q=Deimos+Mosaic)
- **Target Map:** Look for "Deimos Pictorial Map" or "Deimos Viking Global Mosaic".
- *Note:* Direct permanent links for Deimos textures on USGS are currently fluctuating. The search link provides the most up-to-date access to available products.

### Option 3: NASA Public Domain (Various resolutions)

NASA provides public domain textures but they require more processing:

**Jupiter's Moons:**
- Io: https://astrogeology.usgs.gov/search/map/Io/Voyager-Galileo/Io_Voyager_GalileoSSI_Global_Mosaic_ClrMerge_1km
- Europa: https://astrogeology.usgs.gov/search/map/Europa/Voyager-Galileo/Europa_Voyager_GalileoSSI_Global_ClrMosaic_1km
- Ganymede: https://astrogeology.usgs.gov/search/map/Ganymede/Voyager-Galileo/Ganymede_Voyager_GalileoSSI_Global_ClrMosaic_1km
- Callisto: https://astrogeology.usgs.gov/search/map/Callisto/Voyager-Galileo/Callisto_Voyager_GalileoSSI_Global_Mosaic_1km

**Saturn's Moons:**
- Titan: https://astrogeology.usgs.gov/search/map/Titan/Cassini/Global-Mosaic/Titan_ISS_P19658_Mosaic_Global_4km
- Enceladus: https://astrogeology.usgs.gov/search/map/Enceladus/Cassini/Enceladus_Cassini_mosaic_global_110m
- Other Saturn moons: Check USGS Astrogeology

**Mars Moons:**
- Phobos: https://astrogeology.usgs.gov/search/map/Phobos/Viking/Phobos_Viking_Mosaic_40ppd
- Deimos: https://astrogeology.usgs.gov/search/map/Deimos/Viking/Deimos_Viking_Mosaic_global_4ppd

**Uranus/Neptune Moons:**
- Miranda: https://astrogeology.usgs.gov/search/map/Miranda/Voyager/Miranda_Voyager2_Mosaic_Global_500m
- Triton: https://astrogeology.usgs.gov/search/map/Triton/Voyager/Triton_Voyager2_ClrMosaic_GlobalFill_300mpp

### Option 3: Procedural Textures (Current Fallback)

The game currently uses procedurally generated placeholder textures for these moons. They work but lack the detail of real mission data. The procedural system applies variations to make each moon look different.

## Recommended Approach

1. Download from Solar System Scope for convenience and consistency
2. They're all the same resolution and format
3. Requires attribution in your project: "Textures provided by Solar System Scope (https://www.solarsystemscope.com/)"
4. Already added to README.md

## Installation

1. Download the texture files
2. Place them in `assets/textures/celestial/moons/` or appropriate subdirectory
3. The game will automatically load them instead of the placeholders
4. No code changes needed - the texture priority system handles it automatically
