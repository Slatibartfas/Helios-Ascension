# Additional Celestial Bodies - Texture Expansion Guide

This document outlines additional celestial bodies that have textures available from NASA public domain sources and could be added to the game.

## Currently Have Textures (24 bodies)

### Stars (1)
- ✅ Sun (8K)

### Planets (8)
- ✅ Mercury (8K)
- ✅ Venus Surface (8K)
- ✅ Venus Atmosphere (2K)
- ✅ Earth (8K)
- ✅ Mars (8K)
- ✅ Jupiter (8K)
- ✅ Saturn (8K)
- ✅ Uranus (2K)
- ✅ Neptune (2K)

### Moons (12)
- ✅ Moon/Luna - Earth's moon (8K)
- ✅ Io - Jupiter (1K)
- ✅ Europa - Jupiter (1K)
- ✅ Ganymede - Jupiter (1K)
- ✅ Callisto - Jupiter (1K)
- ✅ Titan - Saturn (1K)
- ✅ Rhea - Saturn (1K)
- ✅ Iapetus - Saturn (1K)
- ✅ Dione - Saturn (1K)
- ✅ Enceladus - Saturn (1K)
- ✅ Tethys - Saturn (1K)

### Dwarf Planets (3)
- ✅ Pluto (2K)
- ✅ Ceres (2K)
- ✅ Eris (2K)

## Available from NASA - Could Be Added

### Mars Moons (2) - HIGH PRIORITY
NASA has public domain textures for Mars' moons:

**Phobos**
- Source: NASA Viking mission + Mars Reconnaissance Orbiter
- Resolution: 2K-4K available
- NASA 3D Resources: https://science.nasa.gov/resource/phobos-mars-moon-3d-model/
- Texture: https://science.nasa.gov/3d-resources/mars-phobos/

**Deimos**
- Source: NASA Viking mission + Mars Reconnaissance Orbiter
- Resolution: 2K available
- NASA 3D Resources: https://science.nasa.gov/resource/deimos-mars-moon-3d-model/

### Jupiter Moons (Additional 75 beyond the 4 Galilean)
Most small Jupiter moons lack detailed surface imagery. Available options:
- **Amalthea** - Voyager/Galileo imagery (low resolution)
- **Himalia** - Limited imagery available
- **Most others** - Would require generic/procedural textures

**Recommendation**: Generic "rocky moon" textures for variety

### Saturn Moons (Additional 77 beyond current 6)
Available NASA textures:
- **Mimas** - Cassini imagery, 2K possible
- **Tethys** - Already have ✅
- **Dione** - Already have ✅
- **Rhea** - Already have ✅
- **Iapetus** - Already have ✅
- **Phoebe** - Cassini close-up, 1K-2K available
- **Hyperion** - Cassini imagery available
- **Pan** - Limited imagery
- **Prometheus** - Limited imagery
- **Pandora** - Limited imagery

**Recommendation**: Add Mimas, Phoebe, Hyperion if desired

### Uranus Moons (27 total, 0 currently have textures)
Limited high-resolution imagery from Voyager 2:
- **Miranda** - Best imagery available, 1K possible
- **Ariel** - Limited Voyager 2 imagery
- **Umbriel** - Limited Voyager 2 imagery
- **Titania** - Limited Voyager 2 imagery
- **Oberon** - Limited Voyager 2 imagery

**Source**: NASA Voyager 2 mission (1986 flyby)  
**NASA Resources**: https://science.nasa.gov/uranus/

**Recommendation**: Add Miranda (best quality), consider others if acceptable quality

### Neptune Moons (14 total, 0 currently have textures)
- **Triton** - Best imagery from Voyager 2, 2K possible
- **Others** - Very limited imagery

**Source**: NASA Voyager 2 mission (1989 flyby)  
**NASA Resources**: https://science.nasa.gov/neptune/

**Recommendation**: Add Triton

### Asteroids (145 in game data)
NASA has detailed textures for visited asteroids:

**Available High-Resolution Textures**:
- **Vesta** - NASA Dawn mission, 4K available
  - Source: https://science.nasa.gov/3d-resources/
- **Bennu** - NASA OSIRIS-REx, 5cm/pixel resolution
  - Source: https://astrogeology.usgs.gov/search/map/bennu_osiris_rex_ocams_global_pan_mosaic_5cm
- **Eros** - NEAR Shoemaker mission
- **Itokawa** - Japanese Hayabusa mission
- **Ryugu** - Japanese Hayabusa2 mission

**Generic Asteroid Types Needed**:
1. **C-type (Carbonaceous)** - Dark, carbon-rich asteroids (~75% of asteroids)
2. **S-type (Silicaceous)** - Stony asteroids (~17% of asteroids)
3. **M-type (Metallic)** - Metal-rich asteroids (~8% of asteroids)

**Recommendation**: 
- Add Vesta and Bennu textures (high quality, public domain)
- Create 3 generic procedural/composite textures for C/S/M types
- Apply generics to other asteroids based on their spectral classification

### Comets (20 in game data)
Very limited detailed surface imagery:

**Available**:
- **67P/Churyumov-Gerasimenko** - ESA Rosetta mission
  - Note: ESA imagery, check license (typically CC BY-SA)
  - High-resolution shape model available
- **81P/Wild 2** - NASA Stardust flyby
- **9P/Tempel 1** - NASA Deep Impact
- **103P/Hartley 2** - NASA EPOXI

**Generic Comet Textures Needed**:
1. **Icy/Rocky Nucleus** - Generic comet surface
2. **Dusty/Dark Nucleus** - Low albedo comets

**Recommendation**: 
- Use 67P texture if ESA license acceptable
- Create 1-2 generic comet nucleus textures
- Apply to all comets (they're small and far away)

## Implementation Priority

### Phase 1: Mars Moons (Easy Wins)
- [ ] Download Phobos texture from NASA (2K-4K)
- [ ] Download Deimos texture from NASA (2K)
- [ ] Add to `assets/textures/celestial/moons/`
- [ ] Update `solar_system.ron` with texture paths
- [ ] Total addition: ~5-10MB

### Phase 2: Major Asteroid Textures
- [ ] Download Vesta texture from NASA Dawn (4K)
- [ ] Download Bennu texture from NASA OSIRIS-REx
- [ ] Add to `assets/textures/celestial/asteroids/`
- [ ] Update asteroid entries in `solar_system.ron`
- [ ] Total addition: ~20-30MB

### Phase 3: Generic Textures
- [ ] Create or source 3 generic asteroid types (C/S/M)
- [ ] Create or source 2 generic comet nucleus textures
- [ ] Add to `assets/textures/celestial/asteroids/` and `comets/`
- [ ] Total addition: ~10-20MB

### Phase 4: Additional Moons (Optional)
- [ ] Triton (Neptune) - 2K from Voyager 2
- [ ] Miranda (Uranus) - 1K from Voyager 2
- [ ] Mimas (Saturn) - 2K from Cassini
- [ ] Phoebe (Saturn) - 1K-2K from Cassini
- [ ] Total addition: ~10-20MB

## Total Potential Expansion
- Current: 64MB (24 bodies)
- After Phase 1-4: ~110-170MB (30+ bodies + generics)

## NASA Resources Summary

All these textures are **PUBLIC DOMAIN** from NASA (no attribution required):
- NASA 3D Resources: https://science.nasa.gov/3d-resources/
- NASA GitHub: https://github.com/nasa/NASA-3D-Resources
- USGS Astrogeology: https://astrogeology.usgs.gov/
- NASA SVS: https://svs.gsfc.nasa.gov/

## Notes

1. **Quality Trade-off**: NASA public domain textures are typically 2K-4K, not 8K like Solar System Scope
2. **License**: All NASA sources are public domain (no attribution needed)
3. **Work Required**: NASA textures may need processing (format conversion, projection adjustment)
4. **Storage**: Adding all recommended textures would increase total size to ~110-170MB

## Recommendation

**For the user's request**:
1. **Definitely add**: Phobos, Deimos (Mars moons) - Easy, high quality available
2. **Strongly consider**: Vesta, Bennu (asteroids) - Mission-quality textures available
3. **Consider**: Generic asteroid/comet textures for variety
4. **Optional**: Additional Saturn/Uranus/Neptune moons if storage acceptable

All additions would be NASA public domain (no attribution needed), complementing our current CC BY 4.0 Solar System Scope textures.
