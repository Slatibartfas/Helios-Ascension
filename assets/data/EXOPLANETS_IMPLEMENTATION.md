# Exoplanet Data for Implementation

This file contains exoplanet data from the NASA Exoplanet Archive that can be added to `nearest_stars_raw.json`.

## Overview

- **Total exoplanet hosts within 50 pc (~163 ly):** 590 stars
- **Total with confirmed planets:** ~550+ unique systems
- **Planet count:** 5,000+ confirmed exoplanets in this range

---

## Priority 1: Stars Already in Game (Verify Data)

| Star | Distance | Existing Planets | Verify |
|------|----------|------------------|--------|
| Proxima Centauri | 4.2 ly | b, d | Compare with NASA (6 planets) |
| Barnard's Star | 6.0 ly | b | NASA has 4-5 planets |
| Epsilon Eridani | 10.4 ly | b | Check NASA data |
| Tau Cet | 11.8 ly | e, f, g, h | NASA has 4 planets |
| Ross 128 | 11.0 ly | b | NASA has 1 |
| Kapteyn's Star | 12.8 ly | b | NASA has 1 |

---

## Priority 2: High-Profile Stars to Add Next

### Famous Historical Discoveries

| Star | Distance | Planets | Why Add |
|------|----------|---------|---------|
| **51 Pegasi** | 50.4 ly | 1 (b) | First exoplanet around Sun-like star (1995) |
| **47 Ursae Majoris** | 45.0 ly | 3 (b,c,d) | Solar system analog |
| **61 Virginis** | 27.7 ly | 3 (b,c,d) | Close multi-planet system |
| **55 Cancri** | 41.0 ly | 5 (b,c,d,e,f) | Famous 5-planet system |

### Notable Multi-Planet Systems (within 25 ly)

| Star | Distance | Planets | Notes |
|------|----------|---------|-------|
| **GJ 876** | 15.2 ly | 18 planets | Red dwarf, very active |
| **HD 219134** | 21.3 ly | 30 planets | Record holder for nearby multi-planet |
| **AU Mic** | 31.7 ly | 26 planets | Young star with debris disk |
| **GJ 887** | 10.7 ly | 8 planets | Bright red dwarf |
| **YZ Cet** | 12.1 ly | 8 planets | Ultra-cool dwarf |
| **Teegarden's Star** | 12.5 ly | 5 planets | Very low mass |
| **Wolf 1061** | 14.0 ly | 6 planets | Close habitable zone |
| **L 98-59** | 34.1 ly | 5 planets | TESS discovery |

---

## Complete Star List (Within 50 pc / 163 ly)

Sorted by distance. Target: 6000 stars eventually, this gives you ~590 high-value targets.

### Within 15 ly (Priority)

| Distance | Name | Planets | Temp (K) | Mass (M☉) | Status |
|----------|------|---------|----------|-----------|--------|
| 4.2 ly | Proxima Centauri | 6 | 2900 | 0.122 | In game |
| 6.0 ly | Barnard's Star | 4-5 | 3195 | 0.162 | In game |
| 10.4 ly | Epsilon Eridani | 1 | - | 0.820 | In game |
| 10.7 ly | GJ 887 | 8 | 3688 | 0.495 | Add |
| 11.0 ly | Ross 128 | 1 | 3192 | 0.168 | In game |
| 11.5 ly | GJ 15 A | 2 | 3607 | 0.380 | Add |
| 11.8 ly | Tau Ceti | 4 | - | 0.783 | In game |
| 11.9 ly | Epsilon Indi A | 1 | 4760 | 0.760 | In game |
| 12.0 ly | GJ 1061 | 3 | 2953 | 0.120 | Add |
| 12.1 ly | YZ Cet | 8 | 3151 | 0.142 | Add |
| 12.5 ly | Teegarden's Star | 5 | 3034 | 0.097 | In game |
| 12.8 ly | Kapteyn's Star | 1 | 3550 | 0.281 | In game |
| 14.0 ly | Wolf 1061 | 6 | 3342 | 0.294 | In game |
| 14.6 ly | GJ 9066 | 1 | 3154 | 0.150 | Add |
| 14.8 ly | GJ 674 | 1 | 3600 | 0.350 | Add |
| 14.8 ly | GJ 687 | 2 | - | 0.400 | In game |
| 15.2 ly | GJ 876 | 18 | - | 0.320 | In game |

### 15-25 ly

| Distance | Name | Planets | Temp (K) | Mass (M☉) |
|----------|------|---------|----------|------------|
| 15.8 ly | GJ 1002 | 2 | 3024 | 0.120 |
| 16.2 ly | GJ 832 | 3 | - | 0.450 |
| 16.3 ly | GJ 682 | 2 | 3028 | 0.270 |
| 17.5 ly | GJ 3323 | 2 | 3159 | 0.164 |
| 18.2 ly | GJ 251 | 2 | 3342 | 0.350 |
| 18.5 ly | GJ 411 | 5 | 3719 | 0.390 |
| 18.8 ly | GJ 229 | 4 | - | 0.509 |
| 19.3 ly | HD 180617 | 2 | 3534 | 0.484 |
| 19.3 ly | GJ 273 | 2 | 3382 | 0.290 |
| 19.6 ly | HD 20794 | 10 | 5368 | 0.790 |
| 20.4 ly | HN Lib | 1 | 3347 | 0.291 |
| 20.4 ly | GJ 896 A | 1 | - | 0.436 |
| 20.5 ly | GJ 581 | 12 | 3500 | 0.295 |
| 20.7 ly | GJ 338 B | 2 | 4014 | 0.640 |
| 21.1 ly | GJ 625 | 1 | 3499 | 0.300 |
| 21.3 ly | HD 219134 | 30 | 4699 | 0.810 |
| 22.4 ly | LTT 1445 A | 2 | - | 0.257 |
| 22.9 ly | GJ 393 | 1 | 3579 | 0.426 |
| 23.6 ly | GJ 4274 | 2 | 3228 | 0.180 |
| 23.6 ly | GJ 667 C | 8 | - | - |
| 24.8 ly | GJ 514 | 1 | 3728 | 0.510 |
| 26.2 ly | GJ 1151 | 1 | 3280 | 0.164 |
| 26.3 ly | GJ 486 | 2 | 3317 | 0.312 |
| 26.6 ly | GJ 686 | 1 | 3656 | 0.426 |
| 27.2 ly | GJ 1289 | 1 | 3296 | 0.210 |
| 27.7 ly | 61 Vir | 8 | 5577 | 0.942 |

### 25-50 ly (Notable systems only - see full list below)

| Distance | Name | Planets | Temp (K) | Mass (M☉) |
|----------|------|---------|----------|------------|
| 28.1 ly | CD Cet | 1 | 3130 | 0.161 |
| 28.7 ly | HD 192310 | 2 | 5166 | 0.800 |
| 28.7 ly | GJ 849 | 2 | 3467 | 0.450 |
| 29.6 ly | GJ 433 | 3 | - | 0.480 |
| 30.3 ly | HD 102365 | 1 | 5630 | 0.850 |
| 30.7 ly | GJ 367 | 3 | 3522 | 0.455 |
| 30.8 ly | GJ 357 | 3 | 3505 | 0.342 |
| 30.9 ly | GJ 3512 | 2 | 3141 | 0.123 |
| 31.5 ly | AU Mic | 26 | 3540 | 0.635 |
| 31.8 ly | GJ 436 | 10 | - | 0.470 |
| 32.6 ly | HD 260655 | 2 | 3803 | 0.439 |
| 34.0 ly | GJ 536 | 2 | 3641 | 0.528 |
| 34.1 ly | L 98-59 | 5 | 3415 | 0.292 |
| 35.2 ly | GJ 86 | 1 | 5182 | 0.930 |
| 35.9 ly | GJ 1148 | 2 | 3287 | 0.344 |
| 36.2 ly | GJ 740 | 1 | 3913 | 0.580 |
| 36.3 ly | HD 3651 | 1 | 5221 | 0.799 |
| 40.7 ly | 55 Cnc | 5 | 5198 | 1.015 |
| 40.7 ly | HD 69830 | 3 | 5385 | 0.860 |
| 41.0 ly | GJ 1132 | 2 | 3229 | 0.195 |
| 45.0 ly | 47 UMa | 3 | 5872 | 1.060 |
| 50.4 ly | 51 Peg | 1 | 5758 | 1.030 |

---

## Implementation Instructions

### Schema for New Stars

```json
{
  "system_name": "61 Virginis",
  "distance_ly": 27.7,
  "stars": [
    {
      "name": "61 Virginis",
      "spectral_type": "G6V",
      "mass_sol": 0.942,
      "radius_sol": 0.963,
      "temp_k": 5577,
      "luminosity_sol": 0.65,
      "planets": [
        {
          "name": "61 Vir b",
          "mass_earth": 5.1,
          "radius_earth": 2.1,
          "period_days": 4.215,
          "semi_major_axis_au": 0.050,
          "eccentricity": 0.05,
          "type": "Super-Earth"
        },
        {
          "name": "61 Vir c",
          "mass_earth": 5.5,
          "radius_earth": 3.9,
          "period_days": 38.1,
          "semi_major_axis_au": 0.217,
          "eccentricity": 0.05,
          "type": "Neptune"
        }
      ],
      "metallicity": -0.01
    }
  ]
}
```

### Mass Conversion

| Unit | Conversion |
|------|------------|
| 1 M_J (Jupiter) | = 317.8 M_E (Earth) |
| 1 M_Neptune | = 17.15 M_E |
| 1 M_Uranus | = 14.5 M_E |

### Planet Type Classification

| Type | Criteria |
|------|----------|
| Hot Jupiter | Period < 10 days, Mass > 50 M_E |
| Warm Jupiter | Period 10-100 days |
| Hot Neptune | Period < 10 days, Mass 10-50 M_E |
| Warm Neptune | Period 10-100 days, Mass 10-50 M_E |
| Super-Earth | Mass 2-10 M_E or Radius 1-2 R_E |
| Mini-Neptune | Radius 2-4 R_E |
| Gas Giant | Mass > 50 M_E |
| Ice Giant | Mass 10-50 M_E, Radius > 4 R_E |

### Luminosity Estimation

Use: `L_sol = R_sol² × (T_eff / 5778)^4`

### Spectral Type Reference

| Type | Temp (K) | Color |
|------|----------|-------|
| G | 5200-6000 | Yellow |
| K | 3700-5200 | Orange |
| M | 2400-3700 | Red |

---

## Full List (590 Stars)

To generate the complete list programmatically:

```python
import csv

hosts = {}
with open('Exoplanets_NASA.csv', 'r') as f:
    reader = csv.reader(f)
    header = None
    for row in reader:
        if row and row[0] == 'rowid':
            header = row
            break
    cols = {h.strip(): i for i, h in enumerate(header)}

    for row in reader:
        host = row[2].strip()
        if row[9] == '1':
            try:
                dist_pc = float(row[cols['sy_dist']])
            except:
                continue
            if dist_pc < 50:
                if host not in hosts:
                    try:
                        hosts[host] = {
                            'name': host,
                            'dist_pc': dist_pc,
                            'dist_ly': dist_pc * 3.26156,
                            'st_teff': row[cols['st_teff']] or '',
                            'st_mass': row[cols['st_mass']] or '',
                            'num_planets': 1
                        }
                    except:
                        pass
                else:
                    hosts[host]['num_planets'] += 1

for h in sorted(hosts.values(), key=lambda x: x['dist_pc']):
    print(f"{h['dist_pc']:.1f} pc | {h['dist_ly']:.1f} ly | {h['num_planets']} planets | {h['st_teff']} K | {h['st_mass']} Msun | {h['name']}")
```

---

## Data Source

NASA Exoplanet Archive: http://exoplanetarchive.ipac.caltech.edu
File: `Exoplanets_NASA.csv` (added 2026-03-05)

---

## Notes

- Many GJ (Gliese-Jahreiss) and HD (Henry Draper) catalog numbers map to common names
- For binary systems (e.g., GJ 725 A/B), add as separate stars in same system
- Some stars have multiple parameter sets - use `default_flag=1` for best values
- Target 6000 stars: ~590 are within 50 pc with known exoplanets
- Remaining ~5400 can be procedural stars following realistic stellar distributions
