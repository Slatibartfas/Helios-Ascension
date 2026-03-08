# Desert Planet Textures

Textures for hot sandy worlds with dunes, mesas, eroded basins, and occasional traces of water.

## Characteristics
- Sand seas, rocky mesas, dry basins, and dust haze
- Yellow, tan, ochre, and light brown tones
- Little vegetation and only sparse surface water features
- Typical procedural band: above 60C and up to 200C

## Runtime Category
- Manifest key: `desert`
- Shares the hot rocky band with `martian`, but represents sandy yellow desert worlds

## Adding Textures

Drop any equirectangular planet texture here and register it in `assets/data/planet_textures.ron`:

```ron
"desert": [
    "textures/celestial/planets/desert/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA Solar System Exploration textures (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom procedural desert-world textures with dunes and escarpments
