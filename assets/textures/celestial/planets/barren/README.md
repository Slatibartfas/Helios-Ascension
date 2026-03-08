# Barren Planet Textures

Textures for dry, rocky, mostly neutral-toned worlds with little atmosphere and no active biosphere.

## Characteristics
- Dry regolith, rock fields, cratered plains, and weathered highlands
- Grey, tan, beige, and muted brown tones
- Minimal or no standing surface water
- Used as the generic fallback rocky category

## Runtime Category
- Manifest key: `barren`
- Used as the default rocky fallback when no more specific terrestrial archetype applies

## Adding Textures

Drop any equirectangular planet texture here and register it in `assets/data/planet_textures.ron`:

```ron
"barren": [
    "textures/celestial/planets/barren/your_texture.jpg",
    // ... existing entries
],
```

## Recommended Sources
- NASA Solar System Exploration textures (Public Domain)
- Solar System Scope (CC BY 4.0) - https://www.solarsystemscope.com/textures/
- Custom procedural dry-rock or cratered-terrain textures
