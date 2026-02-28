use serde::{Deserialize, Serialize};
use bevy::prelude::*;
use crate::astronomy::OceanType;

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
pub fn kelvin_to_color(temperature: f32) -> Color {
    let t = temperature.clamp(1000.0, 40000.0) / 100.0;
    
    let r;
    let g;
    let b;
    
    // Red
    if t <= 66.0 {
        r = 255.0;
    } else {
        r = 329.698727446 * (t - 60.0).powf(-0.1332047592);
    }
    
    // Green
    if t <= 66.0 {
        g = 99.4708025861 * t.ln() - 161.1195681661;
    } else {
        g = 288.1221695283 * (t - 60.0).powf(-0.0755148492);
    }
    
    // Blue
    if t >= 66.0 {
        b = 255.0;
    } else if t <= 19.0 {
        b = 0.0;
    } else {
        b = 138.5177312231 * (t - 10.0).ln() - 305.0447927307;
    }
    
    Color::srgb(
        (r / 255.0).clamp(0.0, 1.0),
        (g / 255.0).clamp(0.0, 1.0),
        (b / 255.0).clamp(0.0, 1.0),
    )
}
