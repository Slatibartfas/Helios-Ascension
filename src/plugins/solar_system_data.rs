use crate::astronomy::OceanType;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Type of celestial body
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BodyType {
    Star,
    Planet,
    /// Gas giants are a sub-type of planets but treated specially in some places
    GasGiant,
    DwarfPlanet,
    Moon,
    Asteroid,
    Comet,
    Ring,
}

/// Spectral/compositional class for asteroids
/// Based on scientific taxonomy from JPL, Asterank, and asteroid surveys
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AsteroidClass {
    /// Carbonaceous (dark, carbon-rich) - ~75% of asteroids
    /// High volatiles: Water, Hydrogen, Ammonia, Methane
    CType,
    /// Silicaceous (stony) - ~17% of asteroids
    /// High silicates: Iron, Aluminum, Silicates, Magnesium
    SType,
    /// Metallic (metal-rich) - ~8% of asteroids
    /// High metals: Nickel-Iron, Copper, Noble Metals, Rare Earths
    MType,
    /// Vestoid (basaltic) - Rare, from Vesta family
    /// High titanium and silicates from differentiated crust
    VType,
    /// Dark/Primitive - Outer belt, very carbon-rich
    /// Extremely high volatiles and organics
    DType,
    /// Primitive - Similar to D-type, outer belt
    /// Very high volatiles, low metal content
    PType,
    /// Unknown/other types
    Unknown,
}

/// Atmospheric gas composition for a celestial body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphericGasData {
    /// Name of the gas (e.g., "N2", "O2", "CO2", "H2", "He", "CH4", "NH3", "Ar")
    pub name: String,
    /// Percentage of the gas in the atmosphere (0.0 to 100.0)
    pub percentage: f32,
}

/// Atmospheric data for a celestial body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphereData {
    /// Surface pressure in millibars (1 bar = 1000 millibars)
    /// For gas giants, this is the pressure at a reference altitude (conventionally 1 bar level)
    pub surface_pressure_mbar: f32,
    /// Average surface temperature in Celsius
    pub surface_temperature_celsius: f32,
    /// List of atmospheric gases
    pub gases: Vec<AtmosphericGasData>,
    /// Whether this is a reference altitude pressure (true for gas giants) or actual surface pressure (false for terrestrial)
    /// Gas giants lack solid surfaces, so their pressure is measured at the conventional 1 bar reference level
    #[serde(default)]
    pub is_reference_pressure: bool,

    // --- Atmospheric scattering parameters (all optional, derived from composition if absent) ---
    /// Scale height in km (how quickly density drops with altitude).
    /// If absent, derived from temperature, gravity, and mean molecular weight.
    #[serde(default)]
    pub scale_height_km: Option<f32>,

    /// Rayleigh scattering colour tint (RGB, normalised).
    /// Controls the "sky colour" produced by molecular scattering.
    /// If absent, defaults to blue-ish tint scaled by surface pressure.
    #[serde(default)]
    pub rayleigh_rgb: Option<(f32, f32, f32)>,

    /// Rayleigh scattering strength multiplier (overrides pressure-derived default).
    #[serde(default)]
    pub rayleigh_strength: Option<f32>,

    /// Mie (aerosol/haze) scattering strength multiplier.
    /// Higher values produce a brighter, hazier atmosphere (e.g. Titan).
    #[serde(default)]
    pub mie_strength: Option<f32>,

    /// Mie asymmetry parameter g (0.0 = isotropic, ~0.76 = forward-scattering typical).
    #[serde(default)]
    pub mie_g: Option<f32>,

    /// Haze / aerosol colour (RGB, normalised). Defaults to warm orange for dusty/hazy worlds.
    #[serde(default)]
    pub haze_color: Option<(f32, f32, f32)>,

    /// Overall intensity multiplier for the atmosphere visual (artistic override).
    #[serde(default)]
    pub atmosphere_intensity: Option<f32>,
}

/// Orbital parameters for a celestial body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbitData {
    /// Semi-major axis in AU
    pub semi_major_axis: f32,
    /// Orbital eccentricity (0 = circular, <1 = elliptical)
    pub eccentricity: f32,
    /// Orbital inclination in degrees
    pub inclination: f32,
    /// Longitude of ascending node (Ω) in degrees
    #[serde(default)]
    pub longitude_ascending_node: f32,
    /// Argument of periapsis (ω) in degrees
    #[serde(default)]
    pub argument_of_periapsis: f32,
    /// Orbital period in Earth days
    pub orbital_period: f32,
    /// Initial angle in degrees (mean anomaly at epoch)
    pub initial_angle: f32,
}

/// Multi-layer texture configuration for advanced rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiLayerTextures {
    /// Base color/albedo texture (day side for planets)
    pub base: String,
    /// Night-side emissive texture (city lights, etc.)
    #[serde(default)]
    pub night: Option<String>,
    /// Cloud/atmosphere layer texture
    #[serde(default)]
    pub clouds: Option<String>,
    /// Blend mode for the clouds texture ("add", "blend", "opaque"). Defaults to "add".
    #[serde(default)]
    pub clouds_blend_mode: Option<String>,
    /// Normal/bump map for surface detail
    #[serde(default)]
    pub normal: Option<String>,
    /// Specular/glossiness map (shininess variation)
    #[serde(default)]
    pub specular: Option<String>,
}

/// Complete data for a celestial body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CelestialBodyData {
    /// Name of the body
    pub name: String,
    /// Type of body
    pub body_type: BodyType,
    /// Mass in kg
    pub mass: f64,
    /// Radius in km
    pub radius: f32,
    /// RGB color (0.0 to 1.0)
    pub color: (f32, f32, f32),
    /// RGB emissive color (for stars)
    pub emissive: (f32, f32, f32),
    /// Parent body name (None for the sun)
    pub parent: Option<String>,
    /// Orbital parameters (None for the sun)
    pub orbit: Option<OrbitData>,
    /// Rotation period in Earth days (negative for retrograde)
    pub rotation_period: f32,
    /// Axial tilt in degrees (obliquity to orbit)
    /// For retrograde rotators (Venus, Uranus, Pluto), use values > 90°
    /// so that the tilt itself encodes retrograde — keep rotation_period positive.
    #[serde(default)]
    pub axial_tilt: f32,
    /// Right ascension of the north pole in degrees (direction the tilt points).
    /// Gives each body a unique rotation axis orientation in 3D space.
    /// 0° = tilts toward vernal equinox direction, 90° = tilts 90° around ecliptic, etc.
    #[serde(default)]
    pub north_pole_ra: f32,
    /// Optional texture path (relative to assets directory)
    #[serde(default)]
    pub texture: Option<String>,
    /// Optional alpha texture path for rings (relative to assets directory).
    /// When provided together with `texture`, both are combined into a runtime RGBA texture.
    #[serde(default)]
    pub ring_alpha_texture: Option<String>,
    /// Multi-layer texture configuration (replaces single texture if present)
    #[serde(default)]
    pub multi_layer_textures: Option<MultiLayerTextures>,
    /// Asteroid spectral class (for procedural texture selection)
    #[serde(default)]
    pub asteroid_class: Option<AsteroidClass>,
    /// Atmosphere data (if the body has an atmosphere)
    #[serde(default)]
    pub atmosphere: Option<AtmosphereData>,
    /// Fraction of the surface covered by liquid ocean (0.0–1.0).
    #[serde(default)]
    pub ocean_fraction: Option<f32>,
    /// Type of ocean liquid (Water, Methane, Hydrocarbon, Subsurface, etc.)
    #[serde(default)]
    pub ocean_type: Option<OceanType>,
    /// Average depth of the ocean in km.
    #[serde(default)]
    pub ocean_depth_km: Option<f32>,
    /// Unix timestamp (seconds since Jan 1 1970 UTC) at which this body was permanently
    /// destroyed (e.g. a comet that disintegrated or impacted another body).
    /// When set and the game start timestamp is >= this value, the body is skipped entirely
    /// during loading so it never appears in the simulation.
    /// Leave unset for bodies that are still present in the current era.
    #[serde(default)]
    pub destroyed_at: Option<i64>,
}

/// Complete solar system data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolarSystemData {
    /// List of all celestial bodies
    pub bodies: Vec<CelestialBodyData>,
}

impl SolarSystemData {
    /// Load solar system data from a RON file
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let data: SolarSystemData = ron::from_str(&contents)?;
        Ok(data)
    }

    /// Get a body by name
    pub fn get_body(&self, name: &str) -> Option<&CelestialBodyData> {
        self.bodies.iter().find(|b| b.name == name)
    }

    /// Get all bodies of a specific type
    pub fn get_bodies_by_type(&self, body_type: BodyType) -> Vec<&CelestialBodyData> {
        self.bodies
            .iter()
            .filter(|b| b.body_type == body_type)
            .collect()
    }

    /// Get all children of a parent body
    pub fn get_children(&self, parent_name: &str) -> Vec<&CelestialBodyData> {
        self.bodies
            .iter()
            .filter(|b| b.parent.as_deref() == Some(parent_name))
            .collect()
    }
}

// Visualization scale factors
// Increased scale for planets to be easily visible and clickable
pub const RADIUS_SCALE: f32 = 0.01;
// Minimum size to ensure small moons are visible and clickable
pub const MIN_VISUAL_RADIUS: f32 = 5.0;
// Smaller minimum for asteroids/comets so belts don't look like dense clumps
pub const MIN_VISUAL_RADIUS_ASTEROID: f32 = 0.12;
// Sun needs a separate, smaller scale to not engulf the inner system when planets are oversized
pub const STAR_RADIUS_SCALE: f32 = 0.00015;

/// Calculates the visual radius of a celestial body based on its type and physical radius (km).
/// Applies non-linear scaling to ensure visibility of smaller bodies without making large ones overwhelming.
///
/// Asteroids and comets use a much smaller minimum visual radius so that dense
/// belts don't turn into overlapping blobs.
pub fn calculate_visual_radius(body_type: BodyType, radius_km: f32) -> f32 {
    if body_type == BodyType::Star {
        (radius_km * STAR_RADIUS_SCALE).max(MIN_VISUAL_RADIUS)
    } else {
        // Apply non-linear scaling for planets/moons to improve visibility balance
        // We use a power function (radius^0.65) normalized to Earth's size.
        // This ensures:
        // 1. Order is preserved (larger bodies appear larger)
        // 2. Small bodies are boosted in size (better visibility)
        // 3. Large bodies (Jupiter/Saturn) are dampened (don't look overwhelmingly huge)
        let earth_radius = 6371.0;
        let base_size = earth_radius * RADIUS_SCALE;

        // Asteroids and comets use a much smaller scale and minimum to avoid
        // visual clumping in belts; a steeper power (0.45) compresses their range
        // further and a 0.25× multiplier keeps them appropriately tiny.
        // Planets/moons use 0.65 power to compress dynamic range while keeping
        // relative sizes meaningful.
        let (scale_mult, power, min_radius) = match body_type {
            BodyType::Asteroid | BodyType::Comet => (0.25, 0.45_f32, MIN_VISUAL_RADIUS_ASTEROID),
            _ => (1.0, 0.65, MIN_VISUAL_RADIUS),
        };
        let relative_size = (radius_km / earth_radius).powf(power);
        (base_size * relative_size * scale_mult).max(min_radius)
    }
}

/// Computes a visual-size scale factor for non-Sol star systems.
///
/// In compact systems around dim stars (brown dwarfs, late-M dwarfs),
/// planetary orbits can be 10–100× smaller than in the Sol system.
/// Without scaling, planet meshes appear disproportionately large
/// compared to their orbits.  This function returns a multiplier
/// (0.15 … 1.0) based on the host star's luminosity — a convenient
/// proxy for system extent — and should be applied to
/// `calculate_visual_radius` results for non-star bodies.
///
/// ```text
/// L = 1.0  (Sol)        →  1.0
/// L = 0.5  (α Cen B)    →  ~0.90
/// L = 0.001 (Wolf 359)  →  ~0.35
/// L = 3e-5 (Luhman 16)  →  ~0.21
/// ```
pub fn system_visual_scale(star_luminosity_sol: f32) -> f32 {
    // Habitable-zone distance scales as √L, so orbit sizes (in AU)
    // shrink rapidly for low-luminosity stars.  We use L^0.15 as a
    // dampened proxy so that very faint systems still get meaningful
    // reduction without becoming invisible.
    star_luminosity_sol.max(1e-7).powf(0.15).clamp(0.15, 1.0)
}

/// Convert temperature (Kelvin) to approximate sRGB color.
/// Based on Tanner Helland's algorithm.
#[allow(clippy::excessive_precision)] // constants match published algorithm coefficients exactly
pub fn kelvin_to_color(temperature: f32) -> Color {
    let t = temperature.clamp(1000.0, 40000.0) / 100.0;

    // Red
    let r = if t <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)
    };

    // Green
    let g = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };

    // Blue
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };

    Color::srgb(
        (r / 255.0).clamp(0.0, 1.0),
        (g / 255.0).clamp(0.0, 1.0),
        (b / 255.0).clamp(0.0, 1.0),
    )
}

#[cfg(test)]
mod solar_system_data_tests {
    use super::{BodyType, SolarSystemData};

    #[test]
    fn test_solar_system_data_loads() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Should have 377+ bodies now!
        assert!(
            data.bodies.len() >= 370,
            "Expected at least 370 bodies, got {}",
            data.bodies.len()
        );

        // Check for specific bodies
        assert!(data.get_body("Sol").is_some(), "Sol should exist");
        assert!(data.get_body("Earth").is_some(), "Earth should exist");
        assert!(data.get_body("Moon").is_some(), "Moon should exist");
        assert!(data.get_body("Jupiter").is_some(), "Jupiter should exist");
        assert!(data.get_body("Pluto").is_some(), "Pluto should exist");

        // Verify body types
        let planets = data.get_bodies_by_type(BodyType::Planet);
        assert_eq!(
            planets.len(),
            4,
            "Should have 4 rocky planets (Mercury, Venus, Earth, Mars)"
        );

        let gas_giants = data.get_bodies_by_type(BodyType::GasGiant);
        assert_eq!(
            gas_giants.len(),
            4,
            "Should have 4 gas giants (Jupiter, Saturn, Uranus, Neptune)"
        );

        let stars = data.get_bodies_by_type(BodyType::Star);
        assert_eq!(stars.len(), 1, "Should have 1 star");

        let moons = data.get_bodies_by_type(BodyType::Moon);
        assert!(
            moons.len() >= 140,
            "Should have at least 140 moons, got {}",
            moons.len()
        );

        let asteroids = data.get_bodies_by_type(BodyType::Asteroid);
        assert!(
            asteroids.len() >= 100,
            "Should have at least 100 asteroids, got {}",
            asteroids.len()
        );

        let dwarf_planets = data.get_bodies_by_type(BodyType::DwarfPlanet);
        assert!(
            dwarf_planets.len() >= 50,
            "Should have at least 50 dwarf planets/KBOs, got {}",
            dwarf_planets.len()
        );

        let comets = data.get_bodies_by_type(BodyType::Comet);
        assert!(
            comets.len() >= 15,
            "Should have at least 15 comets, got {}",
            comets.len()
        );
    }

    #[test]
    fn test_solar_system_hierarchy() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Earth should be a child of Sol
        let earth = data.get_body("Earth").expect("Earth should exist");
        assert_eq!(earth.parent.as_deref(), Some("Sol"));

        // Moon should be a child of Earth
        let moon = data.get_body("Moon").expect("Moon should exist");
        assert_eq!(moon.parent.as_deref(), Some("Earth"));

        // Jupiter should have multiple moons
        let jupiter_moons = data.get_children("Jupiter");
        assert!(
            jupiter_moons.len() >= 50,
            "Jupiter should have at least 50 moons, got {}",
            jupiter_moons.len()
        );

        // Check for specific Jovian moons
        assert!(data.get_body("Io").is_some());
        assert!(data.get_body("Europa").is_some());
        assert!(data.get_body("Ganymede").is_some());
        assert!(data.get_body("Callisto").is_some());
    }

    #[test]
    fn test_orbital_parameters() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Earth's semi-major axis should be approximately 1 AU
        let earth = data.get_body("Earth").expect("Earth should exist");
        let earth_orbit = earth.orbit.as_ref().expect("Earth should have orbit");
        assert!(
            (earth_orbit.semi_major_axis - 1.0).abs() < 0.01,
            "Earth should be ~1 AU from Sun"
        );

        // Earth's orbital period should be approximately 365 days
        assert!(
            (earth_orbit.orbital_period - 365.0).abs() < 1.0,
            "Earth year should be ~365 days"
        );

        // Mars should be farther than Earth
        let mars = data.get_body("Mars").expect("Mars should exist");
        let mars_orbit = mars.orbit.as_ref().expect("Mars should have orbit");
        assert!(
            mars_orbit.semi_major_axis > earth_orbit.semi_major_axis,
            "Mars should be farther than Earth"
        );
    }

    #[test]
    fn test_physical_properties() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Sun should be massive
        let sol = data.get_body("Sol").expect("Sol should exist");
        assert!(sol.mass > 1e30, "Sun should be very massive");

        // Jupiter should be more massive than Earth
        let jupiter = data.get_body("Jupiter").expect("Jupiter should exist");
        let earth = data.get_body("Earth").expect("Earth should exist");
        assert!(
            jupiter.mass > earth.mass * 100.0,
            "Jupiter should be much more massive than Earth"
        );

        // Earth should have reasonable radius
        assert!(
            (earth.radius - 6371.0).abs() < 100.0,
            "Earth radius should be ~6371 km"
        );
    }
}

#[cfg(test)]
mod texture_system_tests {
    use super::{AsteroidClass, BodyType, SolarSystemData};

    #[test]
    fn test_texture_field_deserializes() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Check that Sol has a texture
        let sol = data.get_body("Sol").expect("Sol should exist");
        assert!(sol.texture.is_some(), "Sol should have a dedicated texture");
        assert_eq!(
            sol.texture.as_ref().unwrap(),
            "textures/celestial/stars/sun_8k.jpg"
        );

        // Check that Earth has a texture (single or multi-layer)
        let earth = data.get_body("Earth").expect("Earth should exist");
        if let Some(tex) = &earth.texture {
            assert_eq!(tex, "textures/celestial/planets/earth_8k.jpg");
        } else if let Some(ml) = &earth.multi_layer_textures {
            assert_eq!(ml.base, "textures/celestial/planets/earth_daymap_8k.jpg");
        } else {
            panic!("Earth should have a dedicated texture or multi-layer textures");
        }

        // Check that Moon has a texture
        let moon = data.get_body("Moon").expect("Moon should exist");
        assert!(
            moon.texture.is_some(),
            "Moon should have a dedicated texture"
        );
        assert_eq!(
            moon.texture.as_ref().unwrap(),
            "textures/celestial/moons/moon_8k.jpg"
        );

        // Check Venus uses surface texture (single or multi-layer)
        let venus = data.get_body("Venus").expect("Venus should exist");
        if let Some(tex) = &venus.texture {
            assert_eq!(tex, "textures/celestial/planets/venus_surface_8k.jpg");
        } else if let Some(ml) = &venus.multi_layer_textures {
            assert_eq!(ml.base, "textures/celestial/planets/venus_surface_8k.jpg");
        } else {
            panic!("Venus should have a dedicated texture or multi-layer textures");
        }
    }

    #[test]
    fn test_asteroid_classification_deserializes() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Check that Vesta has a dedicated texture and asteroid class
        let vesta = data.get_body("Vesta").expect("Vesta should exist");
        assert_eq!(vesta.body_type, BodyType::Asteroid);
        assert!(
            vesta.texture.is_some(),
            "Vesta should have a dedicated texture"
        );
        assert_eq!(
            vesta.texture.as_ref().unwrap(),
            "textures/celestial/asteroids/vesta_4k.png"
        );

        // Ceres should have asteroid classification and a dedicated texture
        let ceres = data.get_body("Ceres").expect("Ceres should exist");
        assert!(
            ceres.asteroid_class.is_some(),
            "Ceres should have asteroid classification"
        );
        assert_eq!(
            ceres.asteroid_class.as_ref().unwrap(),
            &AsteroidClass::CType,
            "Ceres should be C-type"
        );
        assert!(
            ceres.texture.is_some(),
            "Ceres should have a dedicated texture"
        );
        assert_eq!(
            ceres.texture.as_ref().unwrap(),
            "textures/celestial/planets/dwarf/4k_ceres.jpg",
            "Ceres texture path should be updated"
        );

        let eris = data.get_body("Eris").expect("Eris should exist");
        assert!(eris.texture.is_some(), "Eris needs a dedicated texture");
        assert_eq!(
            eris.texture.as_ref().unwrap(),
            "textures/celestial/planets/eris_2k.jpg"
        );

        // Haumea and Makemake currently fall back to generic dwarf rendering until
        // dedicated texture assets are added.
        for name in &["Haumea", "Makemake"] {
            let body = data
                .get_body(name)
                .unwrap_or_else(|| panic!("{} should exist", name));
            assert!(
                body.texture.is_none(),
                "{} should currently use generic dwarf rendering",
                name
            );
        }
    }

    #[test]
    fn test_generic_texture_selection_logic() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Count bodies with dedicated textures
        let mut dedicated_count = 0;
        let mut generic_asteroid_count = 0;
        let mut generic_comet_count = 0;
        let mut generic_moon_count = 0;

        for body in &data.bodies {
            if body.texture.is_some() {
                dedicated_count += 1;
            } else {
                // These would get generic textures
                match body.body_type {
                    BodyType::Asteroid => generic_asteroid_count += 1,
                    BodyType::Comet => generic_comet_count += 1,
                    BodyType::Moon => generic_moon_count += 1,
                    _ => {}
                }
            }
        }

        // Verify we have the expected counts
        assert!(
            dedicated_count >= 25,
            "Should have at least 25 bodies with dedicated textures, got {}",
            dedicated_count
        );

        assert!(
            generic_asteroid_count >= 100,
            "Should have at least 100 asteroids using generic textures, got {}",
            generic_asteroid_count
        );

        assert!(
            generic_comet_count >= 15,
            "Should have at least 15 comets using generic textures, got {}",
            generic_comet_count
        );

        assert!(
            generic_moon_count >= 100,
            "Should have at least 100 moons using generic textures, got {}",
            generic_moon_count
        );

        println!("Texture coverage:");
        println!("  Dedicated: {}", dedicated_count);
        println!("  Generic asteroids: {}", generic_asteroid_count);
        println!("  Generic comets: {}", generic_comet_count);
        println!("  Generic moons: {}", generic_moon_count);
        println!(
            "  Total: {}",
            dedicated_count + generic_asteroid_count + generic_comet_count + generic_moon_count
        );
    }

    #[test]
    fn test_asteroid_class_distribution() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        let asteroids = data.get_bodies_by_type(BodyType::Asteroid);

        let mut c_type_count = 0;
        let mut s_type_count = 0;
        let mut m_type_count = 0;
        let mut unknown_count = 0;

        for asteroid in asteroids {
            match &asteroid.asteroid_class {
                Some(AsteroidClass::CType) => c_type_count += 1,
                Some(AsteroidClass::SType) => s_type_count += 1,
                Some(AsteroidClass::MType) => m_type_count += 1,
                // Treat other types (V, D, P, etc) as unknown for this test or just count them
                _ => unknown_count += 1,
            }
        }

        // In the current curated asteroid subset, S-type bodies are the most common
        // explicitly classified asteroids.
        assert!(
            s_type_count > c_type_count,
            "S-type asteroids should be most common in the current catalog"
        );

        println!("Asteroid classification distribution:");
        println!("  C-type: {}", c_type_count);
        println!("  S-type: {}", s_type_count);
        println!("  M-type: {}", m_type_count);
        println!("  Unknown: {}", unknown_count);
    }

    #[test]
    fn test_no_bodies_missing_required_fields() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        for body in &data.bodies {
            // All bodies should have a name
            assert!(!body.name.is_empty(), "Body should have a name");

            // All bodies should have mass > 0
            assert!(body.mass > 0.0, "Body {} should have mass > 0", body.name);

            // All bodies should have radius > 0
            assert!(
                body.radius > 0.0,
                "Body {} should have radius > 0",
                body.name
            );

            // All non-star bodies should have a parent
            if body.body_type != BodyType::Star {
                assert!(
                    body.parent.is_some(),
                    "Non-star body {} should have a parent",
                    body.name
                );
            }

            // All non-star, non-ring bodies should have orbital parameters
            if body.body_type != BodyType::Star && body.body_type != BodyType::Ring {
                assert!(
                    body.orbit.is_some(),
                    "Non-star body {} should have orbital parameters",
                    body.name
                );
            }
        }
    }

    #[test]
    fn test_major_moons_have_textures() {
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Major moons that should have dedicated textures
        let major_moons = [
            "Moon",      // Earth's moon
            "Io",        // Jupiter
            "Europa",    // Jupiter
            "Ganymede",  // Jupiter
            "Callisto",  // Jupiter
            "Enceladus", // Saturn
            "Phobos",    // Mars
            "Deimos",    // Mars
            "Triton",    // Neptune
            "Miranda",   // Uranus
        ];

        let titan = data.get_body("Titan").expect("Titan should exist");
        assert!(
            titan.texture.is_none(),
            "Titan intentionally uses generic rendering plus atmospheric haze until a color texture is added"
        );

        for moon_name in &major_moons {
            let moon = data
                .get_body(moon_name)
                .unwrap_or_else(|| panic!("{} should exist", moon_name));
            assert_eq!(
                moon.body_type,
                BodyType::Moon,
                "{} should be a moon",
                moon_name
            );
            assert!(
                moon.texture.is_some(),
                "{} should have a dedicated texture",
                moon_name
            );
        }
    }
}
