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
- Io: 2k_io.jpg → rename to io_1k.jpg
- Europa: 2k_europa.jpg → rename to europa_1k.jpg
- Ganymede: 2k_ganymede.jpg → rename to ganymede_1k.jpg
- Callisto: 2k_callisto.jpg → rename to callisto_1k.jpg
- Titan: 2k_titan.jpg → rename to titan_1k.jpg
- Enceladus: 2k_enceladus.jpg → rename to enceladus_1k.jpg
- Tethys: 2k_tethys.jpg → rename to tethys_1k.jpg
- Dione: 2k_dione.jpg → rename to dione_1k.jpg
- Rhea: 2k_rhea.jpg → rename to rhea_1k.jpg
- Iapetus: 2k_iapetus.jpg → rename to iapetus_1k.jpg
- Phobos: 2k_phobos.jpg → rename to phobos_2k.jpg
- Deimos: 2k_deimos.jpg → rename to deimos_2k.jpg
- Miranda: 2k_miranda.jpg → rename to miranda_2k.jpg
- Triton: 2k_triton.jpg → rename to triton_2k.jpg
- Mimas: 2k_mimas.jpg → rename to mimas_2k.jpg
- Phoebe: 2k_phoebe.jpg → rename to phoebe_2k.jpg

Place them in the appropriate subdirectories:
- Jupiter/Saturn moons → `celestial/moons/`
- Mars moons → `celestial/moons/`
- Uranus/Neptune moons → `celestial/moons/`

### Option 2: NASA Public Domain (Various resolutions)

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
