# Realistic Resource Abundances - Design Document

## Overview

This document explains the realistic resource abundance system designed to support mining depletion mechanics in Helios Ascension.

## Design Philosophy

Resource abundances are based on actual planetary compositions and geological surveys. This ensures:

1. **Realism**: Players learn actual space resource economics
2. **Strategic Depth**: Rare materials create meaningful choices
3. **Longevity**: Common materials support sustained operations
4. **Depletion Mechanics**: Mining realistically depletes resources

## Abundance Scale

All abundances are fractions of total body mass (0.0 to 1.0):
- 0.30 = 30% of body is this resource
- 0.0001 = 0.01% of body (100 ppm)
- 0.000001 = 0.0001% of body (1 ppm)

## Resource Abundance Ranges

### Construction Materials (Common)

Based on Earth's composition and rocky planet surveys:

**Iron (Fe)**: 15-35% of body composition
- Earth's mantle/core: ~30% iron
- Inner rocky planets: High iron content
- Outer system: 5-20% (less concentrated)
- **Mining Impact**: Abundant, will last through extensive operations

**Silicates (SiO2)**: 25-45% of body composition
- Primary component of planetary crust
- Rocks, sand, minerals
- Everywhere in inner system
- **Mining Impact**: Most abundant, nearly inexhaustible

**Aluminum (Al)**: 5-12% of body composition
- Earth's crust: ~8% aluminum
- Third most abundant crustal element
- Good availability in rocky bodies
- **Mining Impact**: Common, long-lasting resource

**Titanium (Ti)**: 0.3-1% of body composition
- Earth's crust: ~0.6% titanium
- Less common but still accessible
- Valuable for high-strength applications
- **Mining Impact**: Moderate availability, will deplete faster than iron/aluminum

### Volatiles (Location-Dependent)

**Water, Hydrogen, Ammonia, Methane**: 30-70% in outer system, <2% in inner system
- Outer system: Abundant ice and gas
- Inner system: Nearly absent (evaporated)
- Critical for life support and fuel
- **Mining Impact**: Very abundant beyond frost line

### Atmospheric Gases (Variable)

**Nitrogen (N2)**: 0-40% depending on location
- Earth's atmosphere: 78% by volume
- Trapped in ice in outer system (10-20%)
- Some inner planets have nitrogen atmospheres
- **Mining Impact**: Moderate to abundant, location-dependent

**Oxygen (O2)**: 0-15% depending on location
- Free O2 rare (requires life or photolysis)
- Bound in water, silicates (very common)
- Atmospheric mining or chemical extraction
- **Mining Impact**: Moderate availability

**Carbon Dioxide (CO2)**: 0-20% depending on location
- Venus: 96% of atmosphere
- Mars: 95% of atmosphere
- Trapped in ice in outer system
- **Mining Impact**: Moderate to abundant

**Argon (Ar)**: 0-5% depending on location
- Earth: ~1% of atmosphere
- Present in many bodies
- Inert gas applications
- **Mining Impact**: Moderate availability

### Fissile Materials (Very Rare)

Based on crustal abundances:

**Uranium (U)**: 0.0003% (3 ppm)
- Earth's crust: ~2.7 ppm
- Range: 0.0001-0.001% (1-10 ppm)
- Critical for nuclear fission
- **Mining Impact**: RARE - will deplete in decades of heavy use

**Thorium (Th)**: 0.0012% (12 ppm)
- Earth's crust: ~9.6 ppm
- Range: 0.0003-0.003% (3-30 ppm)
- 3-4x more abundant than uranium
- Alternative nuclear fuel
- **Mining Impact**: RARE - slightly better than uranium

### Precious Metals (Extremely Rare)

Based on crustal abundances (much higher in meteorites):

**Gold (Au)**: 0.00004% (0.4 ppm)
- Earth's crust: ~0.004 ppm
- Range: 0.00001-0.0001% (0.1-1 ppm)
- Concentrated in deposits by geological processes
- **Mining Impact**: EXTREMELY RARE - a small asteroid might have only 500 Mt total
- **Depletion**: Will exhaust first, drives interplanetary trade

**Silver (Ag)**: 0.00008% (0.8 ppm)
- Earth's crust: ~0.08 ppm
- Range: 0.00003-0.0003% (0.3-3 ppm)
- More common than gold but still rare
- Electronics and currency applications
- **Mining Impact**: EXTREMELY RARE - will deplete quickly

**Platinum (Pt)**: 0.000005% (0.05 ppb)
- Earth's crust: ~0.005 ppb
- Range: 0.000001-0.00001% (0.01-0.1 ppb)
- Rarest of the precious metals
- Critical for catalysts and advanced tech
- **Mining Impact**: ULTRA RARE - entire asteroid might have <100 Mt
- **Depletion**: Most precious resource, exhausts very quickly

**Note on Asteroid Enrichment:**
- Metallic asteroids can have 10-100x crustal abundances
- This is why asteroid mining is economically viable
- Still rare in absolute terms

### Fusion Fuel (Ultra Rare)

**Helium-3 (He3)**: 0.00001-0.0001%
- Extremely rare on Earth (~0.000005 ppb)
- More abundant on Moon (solar wind implantation)
- Gas giants have significant reserves (atmospheric mining)
- **Mining Impact**: ULTRA RARE - most valuable resource
- **Strategic**: Whoever controls He3 controls fusion power

### Specialty Materials (Rare to Moderate)

**Copper (Cu)**: 0.006% (60 ppm)
- Earth's crust: ~60 ppm
- Range: 0.003-0.01% (30-100 ppm)
- Critical for electronics and conductors
- **Mining Impact**: RARE but not as scarce as precious metals
- **Depletion**: Moderately fast

**Rare Earths (REE)**: 0.02% (200 ppm combined)
- 17 elements combined
- Cerium, Neodymium, etc.
- Critical for magnets, electronics, advanced tech
- **Mining Impact**: RARE - important strategic resource
- **Depletion**: Moderate rate

## Mining Depletion Examples

### Example 1: Earth-like Planet Mining Operation

**Planet Specifications:**
- Mass: 5.972×10²⁴ kg
- Initial state: Pristine

**Resource Inventory:**
```
Iron (30%):        1.79×10¹⁵ Mt
Silicates (40%):   2.39×10¹⁵ Mt
Gold (0.0000005%): 2.99×10⁶ Mt
Uranium (0.0003%): 1.79×10⁷ Mt
```

**Mining Campaign:**
Mining 1.0×10¹² Mt of iron over 100 years (10¹⁰ Mt/year)

**After Mining:**
```
Total mass removed: 1.0×10¹² Mt = 1.0×10²¹ kg (0.017% of planet)
Remaining mass: 5.971×10²⁴ kg

Iron (29.97%):      1.78×10¹⁵ Mt (99.4% remains - barely touched)
Gold (still rare):  2.99×10⁶ Mt (unchanged - not mined)
```

**Conclusion**: Common materials sustain very long operations on large bodies.

### Example 2: Asteroid Gold Mine

**Asteroid Specifications:**
- Mass: 1.0×10¹⁸ kg (small metallic asteroid)
- Gold enrichment: 10x typical (asteroid belt bonus)
- Gold abundance: 0.0000005 (0.00005%)

**Resource Inventory:**
```
Gold: 500 Mt total
```

**Mining Operation:**
Mining 10 Mt/year for 20 years = 200 Mt total

**After 20 Years:**
```
Gold mined: 200 Mt (40% of total deposit)
Gold remaining: 300 Mt
Body mass: 9.998×10¹⁷ kg (0.02% decrease)
Gold abundance: 0.0000003 (40% decrease)
```

**After 50 Years:**
```
Gold mined: 500 Mt (100% of deposit - EXHAUSTED)
Must find new asteroid or pay premium prices
```

**Conclusion**: Rare materials deplete quickly, driving exploration.

### Example 3: Gas Giant Helium-3 Harvesting

**Gas Giant (Jupiter-like):**
- Mass: 1.898×10²⁷ kg
- He3 abundance: 0.00001% (upper atmosphere concentration)

**Resource Inventory:**
```
Helium-3: 1.90×10¹⁴ Mt (seems huge!)
```

**Problem**: Atmospheric mining is difficult and expensive

**Operation**: Harvesting 1.0×10⁶ Mt/year (high-tech operation)

**Longevity**: 190,000 years at this rate

**Reality Check**: 
- Only mining accessible upper atmosphere
- Effective supply might be 1% of total = 1,900 years
- Still excellent compared to rocky bodies

**Conclusion**: Gas giants are ultimate He3 source, strategic importance.

### Example 4: Mars-like Colony Resource Exhaustion

**Mars Colony Scenario:**
- Mining support for 1 million colonists
- 100 years of operation

**Critical Resource: Copper (electronics/infrastructure)**
- Mars mass: 6.39×10²³ kg
- Copper: 0.006% = 3.83×10¹³ Mt total
- Annual need: 1.0×10⁷ Mt (10 Mt per person over 100 years)
- Total mined: 1.0×10⁹ Mt

**Result:**
- Copper mined: 1.0×10⁹ Mt (0.003% of total)
- Plenty remains but accessible deposits might be limited
- Need to prospect and develop new mines periodically

**Lesson**: Even on planet-scale, rare materials require continued exploration.

## Gameplay Implications

### Early Game (First Colonies)
- Mine common materials (Iron, Silicates) from nearby bodies
- Establish basic infrastructure
- Resources seem infinite

### Mid Game (Expansion)
- Rare materials (Copper, Uranium) start to deplete
- Need to prospect new sites
- Trade networks develop
- Asteroid mining becomes profitable

### Late Game (Interplanetary Economy)
- Precious metals nearly exhausted on developed worlds
- He3 from gas giants drives fusion economy
- Remote asteroids valuable for rare materials
- Strategic control of resource-rich systems

### Endgame (Kardashev Advancement)
- Mining entire asteroids for rare materials
- Stellar lifting for exotic resources
- Dyson swarm construction (ultimate resource challenge)

## Technical Implementation

### In-Game Representation

Each deposit stores:
- **Abundance**: Fraction of body mass (0.0-1.0)
- **Accessibility**: Ease of extraction (0.0-1.0)

When mining:
```rust
// Extract resources
let mined_mass_kg = extraction_rate * time_period;
let resource_removed = mined_mass_kg / body_mass;

// Update deposit
deposit.abundance -= resource_removed;
body.mass -= mined_mass_kg;

// Calculate remaining
let remaining_mt = deposit.calculate_megatons(body.mass);
```

### Depletion Tracking

Resource depletion alerts:
- Warning at 50% of initial deposit
- Critical at 25% of initial deposit
- Exhausted at <1% of initial deposit

### Economic Dynamics

Price multipliers based on scarcity:
- Abundant (>50% remaining): 1.0x
- Declining (50-25%): 1.5x
- Scarce (25-10%): 3.0x
- Critical (<10%): 10.0x
- Exhausted (<1%): Market unavailable

## Future Considerations

### Recycling
- Reduce depletion by recovering materials
- 50-90% recovery depending on tech level
- Changes resource economics significantly

### Advanced Extraction
- Better accessibility through tech advancement
- Deeper mining, space elevators, etc.
- Doesn't increase abundance, just accessibility

### Synthesis
- Some materials (Platinum group) eventually synthesizable
- Very expensive, doesn't replace mining initially
- Late-game technology

## Conclusion

This realistic abundance system creates a rich strategic environment:
- **Short-term**: Resources seem abundant
- **Long-term**: Scarcity drives decisions
- **Endgame**: Resource exhaustion forces expansion

Players learn real space resource economics while enjoying engaging gameplay.
