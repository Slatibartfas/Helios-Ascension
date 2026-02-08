# Database Integration Guide

## Overview

Helios Ascension's resource system is designed to work with real astronomical databases. This guide explains how to integrate data from NASA, JPL, Asterank, and other sources.

## Recommended Data Sources

### 1. Asterank (Primary Source for Asteroids)

**URL:** http://www.asterank.com/  
**GitHub:** https://github.com/typpo/asterank

**What it provides:**
- ~600,000 known asteroids
- Spectral type classifications
- Estimated composition
- Economic value calculations
- Mass estimates
- Orbital elements

**Data Format:** CSV files available on GitHub

**Key Fields for Helios:**
```csv
full_name,class,spec,diameter,GM,density,...
(162173) Ryugu,C-type,C,0.900,0.0000000003,1.19,...
(16) Psyche,M-type,M,226.0,0.0014,4.2,...
(4) Vesta,V-type,V,525.4,0.0133,3.456,...
```

**Mapping to Helios:**
- `spec` field → `asteroid_class` (C→CType, M→MType, S→SType, etc.)
- `diameter` → `radius` (divide by 2, convert km to game units)
- `GM` → calculate `mass` (gravitational parameter)
- `full_name` → `name`

### 2. JPL Small-Body Database (SBDB)

**URL:** https://ssd.jpl.nasa.gov/sbdb.cgi  
**API:** https://ssd-api.jpl.nasa.gov/doc/sbdb.html

**What it provides:**
- Most accurate physical parameters
- Precise orbital elements
- Discovery information
- Taxonomy classifications

**API Example:**
```bash
curl "https://ssd-api.jpl.nasa.gov/sbdb.api?sstr=433&full-prec=true"
```

**Response includes:**
```json
{
  "object": {
    "fullname": "433 Eros (A898 PA)",
    "spec_T": "S",
    "diameter": "16.84",
    "GM": "0.0000004463",
    ...
  }
}
```

### 3. NASA Planetary Fact Sheet

**URL:** https://nssdc.gsfc.nasa.gov/planetary/factsheet/

**What it provides:**
- Accurate data for planets and major moons
- Mass in 10^24 kg
- Radius in km
- Density, gravity
- Atmospheric composition

**Use for:**
- Updating special body profiles (Europa, Mars, etc.)
- Verifying realistic values
- Cross-checking procedural generation

### 4. SsODNet (Solar System Object Database Network)

**URL:** http://vo.imcce.fr/webservices/ssodnet/  
**API:** RESTful web service

**What it provides:**
- "ssoCard" for every known solar system object
- Best-estimate parameters aggregated from multiple sources
- Taxonomy data
- Physical properties

**API Example:**
```bash
curl "http://vo.imcce.fr/webservices/ssodnet/resolver/1.0/Ceres?output=json"
```

## Import Workflow

### Step 1: Download Data

**Asterank CSV:**
```bash
# Clone the repository
git clone https://github.com/typpo/asterank.git

# Extract asteroid data
cd asterank/data
# Files: asterank.csv, fulldb.csv
```

**JPL SBDB Batch Query:**
```bash
# For top 100 asteroids
curl "https://ssd-api.jpl.nasa.gov/sbdb_query.api?fields=full_name,diameter,GM,spec_T&sb-kind=a&sb-class=INN&limit=100"
```

### Step 2: Parse and Convert

**Example Rust Parser:**
```rust
use serde::Deserialize;
use csv::Reader;

#[derive(Debug, Deserialize)]
struct AsterankRecord {
    full_name: String,
    spec: Option<String>,  // Spectral type
    diameter: Option<f32>, // km
    #[serde(rename = "GM")]
    gm: Option<f64>,       // km^3/s^2
    density: Option<f32>,  // g/cm^3
}

fn parse_asterank(filename: &str) -> Vec<CelestialBodyData> {
    let mut reader = Reader::from_path(filename).unwrap();
    let mut bodies = Vec::new();
    
    for result in reader.deserialize() {
        let record: AsterankRecord = result.unwrap();
        
        // Convert to Helios format
        let body = CelestialBodyData {
            name: record.full_name,
            body_type: BodyType::Asteroid,
            mass: calculate_mass(record.gm, record.diameter, record.density),
            radius: record.diameter.unwrap_or(1.0) / 2.0,
            asteroid_class: parse_spectral_type(&record.spec),
            // ... other fields
        };
        
        bodies.push(body);
    }
    
    bodies
}

fn parse_spectral_type(spec: &Option<String>) -> Option<AsteroidClass> {
    match spec.as_ref()?.chars().next()? {
        'C' | 'c' => Some(AsteroidClass::CType),
        'S' | 's' => Some(AsteroidClass::SType),
        'M' | 'm' => Some(AsteroidClass::MType),
        'V' | 'v' => Some(AsteroidClass::VType),
        'D' | 'd' => Some(AsteroidClass::DType),
        'P' | 'p' => Some(AsteroidClass::PType),
        _ => Some(AsteroidClass::Unknown),
    }
}

fn calculate_mass(gm: Option<f64>, diameter: Option<f32>, density: Option<f32>) -> f64 {
    if let Some(gm_val) = gm {
        // GM = G * M, so M = GM / G
        // G = 6.674e-11 m^3 kg^-1 s^-2
        // GM in km^3/s^2, convert to m^3/s^2
        let gm_m3 = gm_val * 1e9;
        let g = 6.674e-11;
        gm_m3 / g
    } else if let (Some(d), Some(rho)) = (diameter, density) {
        // Calculate from volume and density
        let radius_m = (d * 1000.0) / 2.0;
        let volume = (4.0/3.0) * std::f32::consts::PI * radius_m.powi(3);
        let density_kg_m3 = rho * 1000.0; // g/cm^3 to kg/m^3
        (volume * density_kg_m3) as f64
    } else {
        // Fallback: assume typical density
        1e15 // kg
    }
}
```

### Step 3: Add to solar_system.ron

**Manual Addition (Top Asteroids):**
```ron
SolarSystemData(
    bodies: [
        // ... existing bodies ...
        
        // 16 Psyche - M-type metallic asteroid
        CelestialBodyData(
            name: "16 Psyche",
            body_type: Asteroid,
            mass: 2.27e19,  // kg, from NASA mission data
            radius: 113.0,   // km
            color: (0.45, 0.45, 0.45),
            emissive: (0.0, 0.0, 0.0),
            parent: Some("Sun"),
            orbit: Some(OrbitData(
                semi_major_axis: 2.923,  // AU
                eccentricity: 0.134,
                inclination: 3.1,
                orbital_period: 1826.0,  // days
                initial_angle: 0.0,
            )),
            rotation_period: 0.17917,  // 4.3 hours
            asteroid_class: Some(MType),  // KEY: Spectral classification
        ),
        
        // 1 Ceres - C-type, now a dwarf planet
        CelestialBodyData(
            name: "Ceres",
            body_type: DwarfPlanet,
            mass: 9.38e20,   // kg
            radius: 473.0,   // km
            color: (0.30, 0.30, 0.30),
            emissive: (0.0, 0.0, 0.0),
            parent: Some("Sun"),
            orbit: Some(OrbitData(
                semi_major_axis: 2.768,
                eccentricity: 0.076,
                inclination: 10.59,
                orbital_period: 1679.0,
                initial_angle: 0.0,
            )),
            rotation_period: 0.3781,  // 9.074 hours
            asteroid_class: Some(CType),  // Special profile overrides this
        ),
        
        // ... more asteroids ...
    ],
)
```

**Automated Import (Future Feature):**
```rust
// In solar_system.rs plugin
fn import_asterank_data() {
    let asteroids = parse_asterank("data/asterank_top_1000.csv");
    
    for asteroid in asteroids {
        spawn_celestial_body(&mut commands, &asteroid);
    }
}
```

## Spectral Type Mapping

### From Database to Game

| Database Value | Helios AsteroidClass | Notes |
|---------------|---------------------|-------|
| C, Ch, Cb, Cg, Cgh | CType | Carbonaceous variants |
| S, Sa, Sk, Sl, Sq, Sr | SType | Silicaceous variants |
| M | MType | Metallic |
| V | VType | Vestoid (basaltic) |
| D | DType | Dark primitive |
| P | PType | Primitive |
| X, Xc, Xe, Xk | MType | X-types are metallic |
| T | DType | Similar to D-type |
| B, F, G | CType | Variations of C-type |
| A | SType | Similar to S-type |
| Q, R | SType | Ordinary chondrite-like |
| L, Ld | Unknown | Rare types |

### Subtypes (Future Enhancement)

```rust
#[derive(Debug, Clone, Copy)]
pub enum AsteroidSubtype {
    // C-type variants
    Ch,  // Hydrated
    Cb,  // Background
    Cg,  // Grayish
    Cgh, // Heated
    
    // S-type variants
    Sa,  // A-like
    Sk,  // K-like
    Sl,  // L-like
    Sq,  // Q-like
    
    // Others
    None,
}
```

## Validation and Quality Control

### Data Integrity Checks

```rust
fn validate_asteroid_data(body: &CelestialBodyData) -> Result<(), String> {
    // Mass reasonableness (10 kg to 10^24 kg)
    if body.mass < 10.0 || body.mass > 1e24 {
        return Err(format!("Mass out of range: {}", body.mass));
    }
    
    // Radius reasonableness (0.1 m to 10,000 km)
    if body.radius < 0.0001 || body.radius > 10000.0 {
        return Err(format!("Radius out of range: {}", body.radius));
    }
    
    // Density check (if calculable)
    let density = calculate_density(body.mass, body.radius);
    if density < 0.5 || density > 8.0 {  // g/cm^3
        warn!("Unusual density for {}: {:.2} g/cm^3", body.name, density);
    }
    
    // Spectral class vs body type
    if body.body_type == BodyType::Asteroid && body.asteroid_class.is_none() {
        warn!("Asteroid {} has no spectral class", body.name);
    }
    
    Ok(())
}

fn calculate_density(mass_kg: f64, radius_km: f32) -> f32 {
    let radius_m = radius_km * 1000.0;
    let volume = (4.0/3.0) * std::f32::consts::PI * radius_m.powi(3);
    let density_kg_m3 = mass_kg / volume as f64;
    (density_kg_m3 / 1000.0) as f32  // Convert to g/cm^3
}
```

### Expected Density Ranges

| Type | Expected Density (g/cm³) | Reason |
|------|-------------------------|--------|
| C-Type | 1.2-2.5 | Porous, water ice |
| S-Type | 2.0-3.5 | Stony, olivine/pyroxene |
| M-Type | 4.0-8.0 | Metallic iron-nickel |
| D/P-Type | 0.8-2.0 | Very porous, ice-rich |
| V-Type | 3.0-3.8 | Basaltic rock |

## Priority Import List

### "Top 100" Most Interesting Asteroids

**By Scientific Interest:**
1. 16 Psyche - M-type, NASA mission target
2. 4 Vesta - V-type, differentiated
3. 1 Ceres - C-type, dwarf planet
4. 433 Eros - S-type, NEAR landing site
5. 25143 Itokawa - S-type, Hayabusa target
6. 162173 Ryugu - C-type, Hayabusa2 target
7. 101955 Bennu - B-type, OSIRIS-REx target
8. 21 Lutetia - M-type, Rosetta flyby
9. 243 Ida - S-type, has moon Dactyl
10. 253 Mathilde - C-type, very dark

**By Economic Value (Asterank Rankings):**
1. Ryugu - $82.76 billion (water, organics)
2. 1989 ML - $13.94 billion
3. Nereus - $4.71 billion
4. ... (see Asterank.com for full list)

**By Accessibility:**
- Near-Earth Asteroids (NEAs)
- Delta-v < 6 km/s
- Frequent launch windows

## Maintenance and Updates

### Regular Data Updates

**Quarterly:**
- Check JPL SBDB for newly characterized objects
- Update spectral classifications
- Add newly discovered asteroids

**Annual:**
- Refresh Asterank economic calculations
- Update with new mission data (OSIRIS-REx, Hayabusa2, Psyche)
- Incorporate new spectroscopic surveys

**Event-Driven:**
- New spacecraft mission results (update target body data)
- Occultation measurements (better size/shape)
- Radar observations (better mass estimates)

## Example: Loading Real Data

```rust
// In main.rs or a data loading plugin
use std::fs::File;
use std::io::BufReader;

fn load_asteroid_database(world: &mut World) {
    let file = File::open("data/asteroids_curated.ron").unwrap();
    let reader = BufReader::new(file);
    let data: SolarSystemData = ron::de::from_reader(reader).unwrap();
    
    world.insert_resource(data);
    info!("Loaded {} asteroids from database", data.bodies.len());
}
```

## Future: Real-Time API Integration

```rust
async fn fetch_asteroid_from_jpl(designation: &str) -> Result<CelestialBodyData, Error> {
    let url = format!("https://ssd-api.jpl.nasa.gov/sbdb.api?sstr={}", designation);
    let response = reqwest::get(&url).await?;
    let json: serde_json::Value = response.json().await?;
    
    // Parse JSON and convert to CelestialBodyData
    Ok(parse_jpl_response(json))
}
```

This enables:
- Player-requested asteroid data
- Dynamic universe population
- Always up-to-date with latest discoveries

## Conclusion

The resource system is designed to seamlessly integrate real astronomical data, providing both scientific accuracy and engaging gameplay. Use the tools and workflows in this guide to populate Helios Ascension with real solar system objects!
