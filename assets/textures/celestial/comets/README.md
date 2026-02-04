# Comet Textures

For comets, we use a fallback approach since detailed comet nucleus textures are limited:

## Current Implementation

Comets use their color data from `solar_system.ron` which provides realistic grey-brown colors typical of comet nuclei.

## Future Enhancement

If you want to add a generic comet nucleus texture:

### Recommended Texture
- Dark, rough surface (similar to 67P/Churyumov-Gerasimenko)
- Grey-brown color (very low albedo ~4%)
- Irregular surface with sublimation features

### Source
The best reference is ESA's Rosetta mission imagery of comet 67P/Churyumov-Gerasimenko:
- **ESA Rosetta Images**: https://www.esa.int/Science_Exploration/Space_Science/Rosetta
- **License**: ESA images are generally available for educational use with attribution

### Creating a Generic Texture
For now, using the procedural color approach provides adequate visual representation until high-quality generic textures are added.
