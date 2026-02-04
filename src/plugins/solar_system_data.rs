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
}

/// Spectral/compositional class for asteroids
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AsteroidClass {
    /// Carbonaceous (dark, carbon-rich) - ~75% of asteroids
    CType,
    /// Silicaceous (stony) - ~17% of asteroids
    SType,
    /// Metallic (metal-rich) - ~8% of asteroids
    MType,
    /// Unknown/other types
    Unknown,
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
    /// Optional texture path (relative to assets directory)
    #[serde(default)]
    pub texture: Option<String>,
    /// Multi-layer texture configuration (replaces single texture if present)
    #[serde(default)]
    pub multi_layer_textures: Option<MultiLayerTextures>,
    /// Asteroid spectral class (for procedural texture selection)
    #[serde(default)]
    pub asteroid_class: Option<AsteroidClass>,
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
    #[allow(dead_code)]
    pub fn get_body(&self, name: &str) -> Option<&CelestialBodyData> {
        self.bodies.iter().find(|b| b.name == name)
    }

    /// Get all bodies of a specific type
    #[allow(dead_code)]
    pub fn get_bodies_by_type(&self, body_type: BodyType) -> Vec<&CelestialBodyData> {
        self.bodies
            .iter()
            .filter(|b| b.body_type == body_type)
            .collect()
    }

    /// Get all children of a parent body
    #[allow(dead_code)]
    pub fn get_children(&self, parent_name: &str) -> Vec<&CelestialBodyData> {
        self.bodies
            .iter()
            .filter(|b| b.parent.as_deref() == Some(parent_name))
            .collect()
    }
}
