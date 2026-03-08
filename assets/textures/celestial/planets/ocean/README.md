# Ocean Planet Textures

Textures for water-dominated worlds with global oceans or near-global ocean coverage.

## Characteristics
- Deep blue water coverage with little exposed land
- Teal, blue, and white cloud-highlight tones
- Shallow shelves, scattered islands, or no major continents
- Typical procedural band: -20C to 60C

## Runtime Category
- Manifest key: `ocean`
- Used for confirmed water-ocean worlds and ocean-dominant habitable bodies

## Adding Textures

Drop any equirectangular planet texture here and register it in `assets/data/planet_textures.ron`:

```ron
"ocean": [
    "textures/celestial/planets/ocean/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA Earth Observatory (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom procedural ocean-world renders
