# Generic Asteroid and Comet Textures

For asteroids and comets, we use a combination of approaches:

## Current Implementation

1. **Fallback to Procedural Colors**: Bodies without specific textures use their color data from `solar_system.ron`
2. **Future Enhancement**: Can add generic rocky/icy textures for variety

## Recommended Generic Textures

If you want to add generic textures for asteroids and comets:

### Asteroids
- Use a generic rocky surface texture (grey, cratered)
- Can be created from photos of asteroids like Vesta, Eros, or Bennu
- Apply random rotation/scaling for variety

### Comets  
- Use a dark, icy nucleus texture
- Based on 67P/Churyumov-Gerasimenko imagery from Rosetta mission
- Dark grey-brown with rough surface features

## Sources for Generic Textures

1. **NASA 3D Resources**: https://nasa3d.arc.nasa.gov/
2. **USGS Astrogeology**: https://astrogeology.usgs.gov/
3. **ESA Image Library**: https://www.esa.int/ESA_Multimedia/Images

## Creating Your Own

For a simple rocky asteroid texture:
```bash
# Use ImageMagick or similar to create a basic rocky texture
# This is just an example - real textures should come from mission data
convert -size 1024x512 xc:grey50 -attenuate 0.3 +noise Gaussian asteroid_generic.jpg
```

For now, the game will use the color values defined in the solar system data file, which provides good visual variety for asteroids and comets.
