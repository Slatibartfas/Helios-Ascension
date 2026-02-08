# Unit Conversion Reference for Resource Generation

## Critical: Understanding Megatons (Mt)

All resource values in Helios Ascension are stored in **Megatons (Mt)**.

### Definition
- **1 Megaton (Mt)** = 1,000,000 metric tons = 10^6 metric tons = 10^9 kg

### Common Conversions

#### From Kilograms (kg) to Megatons (Mt)
```
Mt = kg ÷ 10^9
```
Example: 4.6×10^18 kg = 4.6×10^18 ÷ 10^9 = 4.6×10^9 Mt

#### From Metric Tons to Megatons (Mt)
```
Mt = metric tons ÷ 10^6
```
Example: 600,000,000 metric tons = 6×10^8 ÷ 10^6 = 600 Mt

#### From Cubic Kilometers of Water Ice to Megatons (Mt)
```
Mt = km³ × density(kg/m³) × 10^9 (m³/km³) ÷ 10^9 (kg/Mt)
Mt = km³ × density(kg/m³)
```
For water ice (density ≈ 920 kg/m³):
```
Mt = km³ × 920
```
Example: 5,000,000 km³ = 5×10^6 × 920 = 4.6×10^9 Mt

## Real-World Examples with Correct Conversions

### Mars Water Ice
**Scientific Data**: 5 million cubic kilometers of water ice

**Conversion Steps**:
1. Volume: 5,000,000 km³ = 5×10^6 km³
2. Convert to m³: 5×10^6 km³ × 10^9 m³/km³ = 5×10^15 m³
3. Apply ice density: 5×10^15 m³ × 920 kg/m³ = 4.6×10^18 kg
4. Convert to metric tons: 4.6×10^18 kg ÷ 1000 kg/ton = 4.6×10^15 metric tons
5. **Convert to Megatons: 4.6×10^15 metric tons ÷ 10^6 = 4.6×10^9 Mt** ✅

**Quick formula**: 5×10^6 km³ × 920 kg/m³ = 4.6×10^9 Mt

### Moon Water Ice
**Scientific Data**: 600 million metric tons

**Conversion Steps**:
1. Amount: 600,000,000 metric tons = 6×10^8 metric tons
2. **Convert to Megatons: 6×10^8 ÷ 10^6 = 600 Mt** ✅

### Europa Water Ocean
**Scientific Data**: Body is 85% water by mass, total mass ~4.8×10^22 kg

**Conversion Steps**:
1. Total mass: 4.8×10^22 kg
2. Water fraction: 4.8×10^22 × 0.85 = 4.08×10^22 kg
3. Convert to metric tons: 4.08×10^22 ÷ 1000 = 4.08×10^19 metric tons
4. **Convert to Megatons: 4.08×10^19 ÷ 10^6 = 4.08×10^13 Mt** ✅
5. This is about 40.8 trillion Megatons

**Using code formula**: 
- `body_mass_kg * abundance / 1e9` gives Mt directly
- 4.8×10^22 × 0.85 / 1×10^9 = 4.08×10^13 Mt ✅

## Common Mistakes to Avoid

### ❌ WRONG: Treating Mt as metric tons
```rust
// WRONG! Don't do this!
let mars_water_wrong = 5e6; // Thinking this is "5 million metric tons"
// This is actually 5 million MEGATONS = 5×10^12 metric tons!
```

### ✅ CORRECT: Understanding Mt scale
```rust
// CORRECT!
let mars_water_correct = 4.6e9; // 4.6 billion MEGATONS
// This equals 4.6×10^15 metric tons (correct!)
```

### ❌ WRONG: Not converting units properly
```rust
// WRONG!
let moon_water_wrong = 6e8; // "600 million" but in wrong units
// This is 600 million MEGATONS = 6×10^14 metric tons (way too much!)
```

### ✅ CORRECT: Converting metric tons to Mt
```rust
// CORRECT!
let moon_water_correct = 600.0; // 600 Mt
// This equals 6×10^8 metric tons (correct!)
```

## Verification Checklist

When adding new resource data:

1. ✅ **Identify the source unit**
   - Is it kg, metric tons, km³, or already Mt?

2. ✅ **Convert to kg first** (if not already)
   - km³ → m³ → kg (using density)
   - metric tons → kg (×1000)

3. ✅ **Convert kg to Mt**
   - Mt = kg ÷ 10^9

4. ✅ **Verify the order of magnitude**
   - Earth's mass: ~6×10^15 Mt
   - Mars ice: ~5×10^9 Mt (billions)
   - Moon ice: ~600 Mt (hundreds)
   - Asteroid: ~10^3 to 10^12 Mt (depends on size)

5. ✅ **Test with realistic body masses**
   - Use actual planetary masses in tests
   - Check that fractions make sense (water % should be reasonable)

## Reference Table

| Body | Mass (kg) | Resource | Amount | Correct Mt | Wrong Mt (if confused) |
|------|-----------|----------|---------|------------|------------------------|
| Mars | 6.4×10^23 | Water ice | 5M km³ | 4.6×10^9 Mt | 5×10^6 Mt ❌ |
| Moon | 7.3×10^22 | Water ice | 600M tons | 600 Mt | 6×10^8 Mt ❌ |
| Europa | 4.8×10^22 | Water (85%) | Subsurface ocean | 4.08×10^13 Mt | - |
| Earth | 6.0×10^24 | Mass total | - | 6×10^15 Mt | - |
| Small asteroid | 1×10^15 | Mass total | 1 km diameter | 1×10^6 Mt | - |

## Code Examples

### Using create_deposit_from_absolute_mass
```rust
// For scientifically measured values
// Input is ALREADY in Megatons!

// Mars: 4.6 billion Mt
resources.add_deposit(ResourceType::Water, 
    create_deposit_from_absolute_mass(4.6e9, 0.5, BodyType::Planet));

// Moon: 600 Mt
resources.add_deposit(ResourceType::Water,
    create_deposit_from_absolute_mass(600.0, 0.3, BodyType::Moon));
```

### Using create_deposit_legacy
```rust
// For abundance-based calculations
// Abundance is 0.0 to 1.0 (fraction of body mass)

// Earth: 30% iron abundance
// Earth mass = 6×10^24 kg = 6×10^15 Mt
// Iron = 0.30 × 6×10^15 Mt = 1.8×10^15 Mt
resources.add_deposit(ResourceType::Iron,
    create_deposit_legacy(0.30, 0.7, earth_mass_kg, BodyType::Planet));
```

## Formula Summary

### The Core Formula (in create_deposit_legacy)
```rust
let total_mass_mt = (body_mass_kg * abundance) / 1e9;
```

This converts:
- `body_mass_kg` (in kilograms)
- `abundance` (fraction 0.0-1.0)
- To `total_mass_mt` (in Megatons)

### Why it works:
```
Mt = kg × fraction ÷ 10^9
```

Example with Earth iron:
```
Mt = 6×10^24 kg × 0.05 ÷ 10^9
Mt = 3×10^23 ÷ 10^9
Mt = 3×10^14 Mt (300 trillion Megatons of iron in Earth's crust)
```

## Final Reminder

**Always think in powers of 10:**
- Personal scale: kg (10^0)
- Local scale: metric tons (10^3 kg)
- **Game scale: Megatons (10^6 metric tons = 10^9 kg)**
- Planetary scale: Exatons (10^18 kg)

The game uses Megatons as the standard unit to keep numbers manageable while still representing realistic planetary-scale resources.
