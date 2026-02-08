use serde::{Deserialize, Serialize};

/// Type of celestial body
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BodyType {
    Star,
    Planet,
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
    /// Multi-layer texture configuration (replaces single texture if present)
    #[serde(default)]
    pub multi_layer_textures: Option<MultiLayerTextures>,
    /// Asteroid spectral class (for procedural texture selection)
    #[serde(default)]
    pub asteroid_class: Option<AsteroidClass>,
    /// Atmosphere data (if the body has an atmosphere)
    #[serde(default)]
    pub atmosphere: Option<AtmosphereData>,
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
// Sun needs a separate, smaller scale to not engulf the inner system when planets are oversized
pub const STAR_RADIUS_SCALE: f32 = 0.00015; 

/// Calculates the visual radius of a celestial body based on its type and physical radius (km).
/// Applies non-linear scaling to ensure visibility of smaller bodies without making large ones overwhelming.
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
        // Calculate what Earth's size would be with linear scaling keeping Earth roughly the same relative size
        let base_size = earth_radius * RADIUS_SCALE;
        
        // Relative size factor compared to Earth, raised to power < 1.0 to compress dynamic range
        // Power 0.65 means:
        // - Earth (ratio 1.0) -> 1.0^0.65 = 1.0 (Same size)
        // - Jupiter (ratio ~11) -> 11^0.65 = ~4.75 (Previously ~11x, now ~4.75x)
        // - Moon (ratio ~0.27) -> 0.27^0.65 = ~0.43 (Previously ~0.27x, now ~0.43x - bigger!)
        let relative_size = (radius_km / earth_radius).powf(0.65);
        (base_size * relative_size).max(MIN_VISUAL_RADIUS)
    }
}
