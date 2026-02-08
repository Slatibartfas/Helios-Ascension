# Stellar Metallicity Data Sources

This document describes the metallicity data used for nearby stars in Helios: Ascension.

## What is Metallicity?

Metallicity is a measure of the abundance of elements heavier than hydrogen and helium in a star. It's expressed as [Fe/H], the logarithmic ratio of iron to hydrogen compared to the Sun:

- **[Fe/H] = 0.0**: Solar metallicity (like the Sun)
- **[Fe/H] > 0.0**: Metal-rich (more heavy elements than the Sun)
- **[Fe/H] < 0.0**: Metal-poor (fewer heavy elements than the Sun)

## Why Metallicity Matters

In Helios: Ascension, stellar metallicity affects:
- **Resource abundance** in planets: Metal-rich stars have more rare metals and fissiles
- **Planet formation**: Higher metallicity correlates with more planets
- **System age**: Lower metallicity often indicates older stellar populations

The metallicity multiplier is: `(1.0 + [Fe/H] × 0.6).clamp(0.5, 1.5)`

Example resource abundance:
- Star with [Fe/H] = +0.5 → 1.3× rare metals/fissiles
- Star with [Fe/H] = -0.5 → 0.7× rare metals/fissiles

## Data Sources

Real metallicity values from astronomical databases:

### Primary Sources
1. **SIMBAD Database** (CDS Strasbourg)
   - http://simbad.u-strasbg.fr/
   - Comprehensive stellar parameters database

2. **Hypatia Catalog**
   - https://www.hypatiacatalog.com/
   - Stellar abundance database for FGK stars

3. **Geneva-Copenhagen Survey**
   - http://vizier.u-strasbg.fr/viz-bin/VizieR
   - Detailed metallicities for nearby stars

4. **NASA Exoplanet Archive**
   - https://exoplanetarchive.ipac.caltech.edu/
   - Host star parameters including metallicity

5. **Published Literature**
   - Individual papers for specific stars
   - Review articles on nearby star properties

## Metallicity Values in Helios: Ascension

### Notable Metal-Rich Stars ([Fe/H] > +0.2)
- **Sirius A**: +0.50 (bright A-type star)
- **Alpha Centauri B**: +0.23 (K-dwarf)
- **Alpha Centauri A**: +0.20 (G-dwarf, Sun-like)
- **Proxima Centauri**: +0.10 (M-dwarf)

### Solar Metallicity Stars ([Fe/H] ≈ 0.0)
- **Procyon A**: 0.00 (F-type star)
- **Lacaille 9352**: 0.00 (M-dwarf)
- **Lacaille 8760**: 0.00 (K-dwarf)
- **Ross 128**: -0.02 (nearly solar M-dwarf)

### Metal-Poor Stars ([Fe/H] < -0.3)
- **Kapteyn's Star**: -0.86 (very metal-poor halo star)
- **Barnard's Star**: -0.50 (old M-dwarf)
- **Tau Ceti**: -0.50 (famous G-dwarf)
- **van Maanen's Star**: -0.50 (white dwarf)
- **Ross 154**: -0.40 (M-dwarf)
- **61 Cygni A/B**: -0.40 (K-dwarf binary)

## Estimation Methods

For stars without direct measurements, we estimated metallicity based on:

1. **Spectral Type Correlations**
   - M-dwarfs typically have [Fe/H] = -0.3 to 0.0
   - K-dwarfs typically have [Fe/H] = -0.2 to +0.1
   - G-dwarfs vary widely based on age

2. **Stellar Age Indicators**
   - Older stars (subdwarfs) → lower metallicity
   - Young stars → higher metallicity
   - Halo stars → very low metallicity

3. **Kinematics**
   - High proper motion → older, metal-poor
   - Low proper motion → younger, metal-rich

4. **Companion Stars**
   - Binary stars share the same metallicity
   - Used primary star's value for companions

## Uncertainty

Metallicity measurements have uncertainties:
- Well-studied stars (FGK types): ±0.05 dex
- M-dwarfs: ±0.10 dex
- Brown dwarfs: ±0.20 dex (estimates)
- White dwarfs: ±0.30 dex (progenitor estimates)

For gameplay, we use single values rather than ranges.

## Stars Without Metallicity Data

Stars without real metallicity data (about 33% of the catalog) use randomly generated values in the range [-0.5, +0.5] [Fe/H]. This is logged during generation:

```
INFO: No metallicity data for 'Example Star', using random: -0.23
```

## Distribution of Metallicities

In our catalog of 60 star systems:
- **Metal-rich** ([Fe/H] > +0.1): ~15%
- **Solar** ([Fe/H] = -0.1 to +0.1): ~35%
- **Metal-poor** ([Fe/H] < -0.1): ~50%

This reflects the local stellar neighborhood, which includes both disk stars (higher metallicity) and some halo stars (lower metallicity).

## Future Improvements

1. **More complete data**: Add metallicity for remaining stars
2. **Uncertainty ranges**: Model metallicity as a range rather than single value
3. **Element-specific abundances**: Track [C/Fe], [O/Fe], etc. for more detailed chemistry
4. **Age-metallicity correlation**: Use stellar age to refine estimates
5. **Galactic position effects**: Model metallicity gradients in the galaxy

## References

- Santos, N. C., et al. (2004). "The Planet-Metallicity Correlation" A&A, 415, 1153
- Valenti, J. A., & Fischer, D. A. (2005). "Spectroscopic Properties of Cool Stars" ApJS, 159, 141
- Hinkel, N. R., et al. (2014). "Stellar Abundances in the Solar Neighborhood: The Hypatia Catalog" AJ, 148, 54
- Nordström, B., et al. (2004). "The Geneva-Copenhagen Survey of the Solar Neighbourhood" A&A, 418, 989

## Notes for Save/Load

Metallicity is stored in the JSON data file, not generated at runtime. This ensures:
- **Consistency**: Same stars always have same metallicity
- **Reproducibility**: Different game seeds produce same star properties
- **Scientific accuracy**: Uses real data when available

The GameSeed only affects procedural planet generation, not star properties.
