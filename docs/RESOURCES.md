# Resource System

Complete reference for the resource and economy system in Helios: Ascension.

## Table of Contents

1. [Resource Types](#resource-types)
2. [Realistic Abundances](#realistic-abundances)
3. [Unit System](#unit-system)
4. [Solar System Resources](#solar-system-resources)
5. [Tiered Reserve Model](#tiered-reserve-model)
6. [Scientific Sources](#scientific-sources)

---

## Resource Types

The game features **31 resource types** organized into **8 categories**:

### Volatiles
- **Water (H2O):** Life support, terraforming
- **Hydrogen (H2):** Fuel, industrial use
- **Ammonia (NH3):** Terraforming, fertilizer
- **Methane (CH4):** Fuel, chemical feedstock
- **Phosphorus (P):** Fertilizer, biological essential (~1000 ppm in crust)

### Atmospheric Gases
- **Nitrogen (N2):** Breathable atmospheres
- **Oxygen (O2):** Life support, combustion
- **Carbon Dioxide (CO2):** Terraforming greenhouse gas
- **Argon (Ar):** Industrial inert gas

### Construction Materials
- **Iron (Fe):** Primary structural material (15-35% of rocky bodies)
- **Aluminum (Al):** Lightweight construction (5-12% of crust)
- **Titanium (Ti):** High-strength applications (0.3-1% of crust)
- **Silicates (SiO2):** Glass, ceramics, major rock component (25-45%)
- **Nickel (Ni):** Alloying metal, common in iron meteorites (~80 ppm in crust)
- **Tungsten (W):** Refractory metal, armour, tooling (~1 ppm in crust)
- **Carbon (C):** Structural composites, nanotubes, steel (~200 ppm in crust)

### Fusion Fuel
- **Helium-3 (He3):** Rare fusion fuel (found in gas giants, lunar regolith)
- **Deuterium (D):** Heavy hydrogen, primary fusion fuel (~156 ppm in water)

### Fissiles
- **Uranium (U):** Nuclear fission fuel (~3 ppm in crust)
- **Thorium (Th):** Alternative nuclear fuel (~12 ppm in crust)

### Precious Metals
- **Gold (Au):** High-value applications (~0.004 ppm in crust)
- **Silver (Ag):** Electronics, currency (~0.08 ppm in crust)
- **Platinum (Pt):** Catalysts, high-tech (~0.005 ppb in crust)

### Strategic Materials
- **Copper (Cu):** Electronics, conductors (~60 ppm in crust)
- **Rare Earths (REE):** Advanced technology, magnets (~200 ppm combined)
- **Lithium (Li):** Batteries, fusion blankets (~20 ppm in crust)
- **Sulfur (S):** Chemical industry, fertilizer (~350 ppm in crust)

### Exotic Materials
- **Antimatter (Am̄):** Produced in particle accelerators; fuel for antimatter drives
- **Exotic Matter (Xm):** Negative-energy-density matter for warp drives and wormholes
- **Metamaterials (Mm):** Engineered composites for cloaking, shielding, and sensors
- **Computronium (Qb):** Optimised computational substrate for Culture-level AI minds

---

## Realistic Abundances

Resource abundances are based on real-world planetary compositions to support future mining and depletion mechanics.

### Design Principles

1. **Scarcity Matters:** Not all bodies have all resources - 40% of non-critical resources are randomly absent
2. **Realistic Fractions:** Based on scientific measurements and estimates
3. **Metallicity Bonuses:** Star metallicity ([Fe/H]) affects rare metals and fissiles by ±30%
4. **Body Type Matters:** Composition varies by body type (rocky planet, ice moon, asteroid type)

### Abundance by Body Type

**Rocky Planets (Earth-like):**
- Iron: 30-35% (core + mantle)
- Silicates: 40-45% (mantle + crust)
- Aluminum: 8-10% (crust)
- Water: Variable (0.1-15% depending on location relative to frost line)

**Ice Moons (Europa-like):**
- Water: 60-90% (subsurface ocean + ice shell)
- Silicates: 10-30% (rocky core)
- Ammonia/Methane: 1-5% (trace volatiles)

**Gas Giants (Jupiter-like):**
- Hydrogen: 65-75% *by mass* (commonly misquoted as ~90%, which is volume fraction)
- Helium: 20-26% by mass (not tracked as a game resource)
- Helium-3: 20-35 ppm (He × He3/He4 × 3/4; protosolar He3/He4 ≈ 1.66×10⁻⁴)
- Deuterium: 25-40 ppm (D/H ~2.5×10⁻⁵, scaled by H mass fraction × 2)
- Methane: 0.05-0.3% by mass (enriched above solar C/H)
- Ammonia: 0.01-0.05% by mass (below NH₃ cloud deck)
- Water: 0.1-1% by mass (deep interior; supercritical fluid)
- Nitrogen: trace (~0.001%)
- No solid resources — atmospheric harvesting only

**Ice Giants (Uranus/Neptune-like):**
- Hydrogen: 12-22% by mass (thin H₂/He envelope)
- Water: 40-60% by mass (superionic/ionic interior mantle)
- Methane: 2-5% by mass (atmospheric + interior; source of blue/teal colour)
- Ammonia: 2-5% by mass (interior ices)
- Helium-3: 3-7 ppm (much less total He)
- Deuterium: 35-55 ppm (enriched D/H ~4.4×10⁻⁵ from accreted ices)
- Nitrogen: ~0.1-0.3% by mass

### Asteroid Spectral Classes

Asteroid type determines which resources are richly available versus absent. Not every asteroid of a given type has every resource — the spectral class sets probabilities and concentration ranges.

**C-type (Carbonaceous) — most common in outer belt:**
- Carbon: 3-8% (primary game source)
- Water: 4-7% (hydrated minerals)
- Phosphorus: 0.1-0.3%
- Sulfur: 1-3%
- Nickel: 1-2%
- Deuterium: trace

**S-type (Silicaceous) — common in inner belt:**
- Silicates: 60-70%
- Iron: 15-25%
- Nickel: 0.5-2%
- Sulfur: minor
- Carbon/Tungsten: trace only

**M-type (Metallic) — primary Nickel source:**
- Iron: 75-90%
- Nickel: 5-15% (highest concentration in solar system)
- Tungsten: 0.01-0.1%
- Platinum group metals: high concentrations

**V-type (Basaltic/Vestoid):**
- Silicates: 55-65%
- Iron: 15-20%
- Nickel: 1-3%
- Tungsten: trace

**D-type (Dark/Organic-rich) — outer solar system:**
- Carbon: 5-12%
- Water: 5-10%
- Phosphorus: 0.1-0.3%
- Sulfur: 2-5%

**P-type (Primitive) — trans-Neptunian influenced:**
- Carbon: 4-10%
- Water: 3-8%
- Phosphorus: 0.05-0.2%
- Sulfur: 1.5-4%

---

## Unit System

**All resource values use Megatons (Mt) as the standard unit.**

### Unit Conversions

```
1 Megaton (Mt) = 10^6 metric tons = 10^9 kg
```

**Common Conversions:**
- 1 metric ton = 10^-6 Mt
- 1 kg = 10^-9 Mt
- 1 km³ of water ice ≈ 920,000 Mt (at 920 kg/m³ density)

### Critical Unit Facts

- **Mars water ice:** 4.6 × 10^9 Mt (scientifically measured: 5 million km³)
- **Moon water ice:** 600 Mt (scientifically measured: 600 million metric tons)
- **Europa water:** ~4 × 10^13 Mt (calculated from mass fraction)

**Functions:**
- `create_deposit_from_absolute_mass(total_mt, proven_fraction, body_type)` - For scientifically measured values
- `create_deposit_legacy(abundance_fraction, variability, body_mass_kg, body_type)` - For calculated abundances

---

## Solar System Resources

### Inner Planets

**Mercury:**
- Iron: Very high (60-70% metallic core)
- Silicates: 30-40%
- Water: None (too hot)
- Helium-3: Trace amounts in regolith from solar wind

**Venus:**
- Silicates: 45-50%
- Iron: 30-35%
- CO2: Massive atmospheric reserves
- Sulfur compounds: High

**Earth:**
- Iron: 32% (mostly in core)
- Silicates: 45%
- Water: 0.023% (oceans + ice)
- Aluminum: 8%
- All resource types present

**Mars:**
- Water: 4.6 × 10^9 Mt (polar caps + subsurface)
- Iron: 18% (FeO in crust + core)
- Silicates: 45%
- CO2: Ice caps

### Outer System

**Jupiter:**  (mass 1.898×10²⁷ kg)
- Hydrogen: 71% by mass (Guillot 2005, Galileo probe)
- Helium: 24% by mass (not tracked)
- He-3: 30 ppm (Mahaffy et al. 1998)
- Deuterium: 37 ppm (D/H 2.6×10⁻⁵, Lellouch 2001)
- Methane: ~0.1% by mass (4× solar C/H)
- Ammonia: ~0.05% by mass
- Water: ~0.5% by mass (Juno MWR deep estimate)
- Nitrogen: trace
- No solid deposits

**Saturn:**  (mass 5.683×10²⁶ kg)
- Hydrogen: 69% by mass
- Helium: 25% by mass (outer atmosphere depleted by He rain-out)
- He-3: 25 ppm (outer envelope depleted)
- Deuterium: 29 ppm (D/H 2.1×10⁻⁵)
- Methane: ~0.3% by mass (enriched ~10× solar)
- Ammonia: ~0.02% by mass
- Water: ~0.5% by mass (deep interior)
- No solid deposits

**Uranus:**  (mass 8.681×10²⁵ kg, ice giant)
- Hydrogen: 15% by mass (thin envelope)
- Water: 50% by mass (superionic interior mantle)
- Methane: 3% by mass (teal colour source)
- Ammonia: 3% by mass
- He-3: 5 ppm
- Deuterium: 44 ppm (enriched D/H 4.4×10⁻⁵)

**Neptune:**  (mass 1.024×10²⁶ kg, ice giant)
- Hydrogen: 18% by mass
- Water: 50% by mass (superionic interior mantle)
- Methane: 3% by mass (deep blue colour source)
- Ammonia: 3% by mass
- He-3: 5 ppm
- Deuterium: 41 ppm (D/H 4.1×10⁻⁵)

### Major Moons

**Moon (Earth's):**
- Water: 600 Mt (polar craters)
- Helium-3: 1.1 million metric tons (regolith)
- Silicates: 45%
- Iron: 10-13%
- Titanium: 4% (high in maria)

**Europa (Jupiter):**
- Water: 4.08 × 10^13 Mt (85% of mass)
- Silicates: 15% (rocky core)
- Excellent volatile source

**Titan (Saturn):**
- Methane: Vast liquid lakes and atmosphere
- Nitrogen: Dense atmosphere
- Water ice: Bedrock
- Organics: Complex hydrocarbons

**Enceladus (Saturn):**
- Water: Active geysers
- Ammonia: Trace
- Silicates: Rocky core

---

## Tiered Reserve Model

Resources are stored in three tiers representing different extraction difficulties:

### Tier 1: Proven Crustal (Easy)
- Surface to 5 km depth
- Currently known deposits
- Easy extraction with basic technology
- Typically 0.1-5% of total reserves

**Extraction:**
- Cost: Low
- Tech requirement: Basic
- Rate: Fast

### Tier 2: Deep Deposits (Medium)
- 5-100 km depth
- Estimated via geological surveys
- Requires advanced drilling
- Typically 5-20% of total reserves

**Extraction:**
- Cost: Medium
- Tech requirement: Advanced drilling
- Rate: Moderate

### Tier 3: Planetary Bulk (Hard)
- 100 km to core
- Calculated from planetary composition
- Requires extreme technology
- Typically 75-95% of total reserves

**Extraction:**
- Cost: Very high
- Tech requirement: Deep mantle mining or core tapping
- Rate: Slow

### Example: Earth Iron

```rust
ResourceReserve {
    proven_crustal: 1.0e6 Mt,      // Known ore deposits
    deep_deposits: 1.0e7 Mt,       // Upper mantle estimates
    planetary_bulk: 1.9e12 Mt,     // Core iron (inaccessible with current tech)
}
```

---

## Scientific Sources

### Primary Sources

**Planetary Compositions:**
- Lodders & Fegley (1998): "The Planetary Scientist's Companion"
- Taylor & McLennan (2009): "Planetary Crusts"
- Morgan & Anders (1980): "Chemical composition of Earth, Venus, and Mercury"

**Mars Water:**
- Dundas et al. (2018): "Exposed subsurface ice sheets in the Martian mid-latitudes"
- NASA Mars Reconnaissance Orbiter data
- Measured: 5 million km³ = 4.6 × 10^9 Mt

**Lunar Resources:**
- Colaprete et al. (2010): "Detection of Water in the LCROSS Ejecta Plume"
- Li et al. (2018): "Direct evidence of surface exposed water ice in the lunar polar regions"
- Measured: 600 million metric tons = 600 Mt

**Asteroid Compositions:**
- DeMeo & Carry (2014): "Solar System evolution from compositional mapping of the asteroid belt"
- Bus-DeMeo Taxonomy
- Spectroscopy data from multiple surveys

**Gas Giant Compositions:**
- NASA Juno mission (Jupiter)
- Cassini mission (Saturn)
- Atreya et al. (2016): "Deep atmosphere composition"

**Helium-3:**
- Wittenberg et al. (1986): "Lunar source of 3He for commercial fusion power"
- Kulcinski et al. (1989): "Fusion energy from the Moon"

### Metallicity Effects

- Santos et al. (2004): "The Planet-Metallicity Correlation"
- Fischer & Valenti (2005): "The Planet-Metallicity Correlation"
- Gonzalez (1997): "The stellar metallicity-giant planet connection"

### Chemical Abundances

- Lodders (2003): "Solar System Abundances and Condensation Temperatures"
- Anders & Grevesse (1989): "Abundances of the elements"
- Palme & O'Neill (2014): "Cosmochemical Estimates of Mantle Composition"

---

## Game Implementation Notes

### Resource Generation Process

1. **Body Type Determination:** Planet, moon, asteroid, or comet
2. **Base Abundances:** Assign realistic fractions based on body type
3. **Metallicity Bonus:** Apply star [Fe/H] multiplier to rare metals and fissiles
4. **Variation:** 40% of non-critical resources randomly absent
5. **Tier Distribution:** Split total into proven/deep/bulk using body type formulas
6. **Special Cases:** Mars, Moon, Europa use absolute mass values

### Code References

- `src/economy/generation.rs` - Resource generation logic
- `src/economy/components.rs` - ResourceReserve and deposit structures
- `src/economy/types.rs` - ResourceType enum and categories

### Validation

Resource values are validated against scientific literature in integration tests:
- Mars water: 4.6 × 10^9 Mt (±10%)
- Moon water: 600 Mt (±50%)
- Earth-like iron fractions: 30-35%
- Asteroid water percentages: Match spectral classes
