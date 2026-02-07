use bevy::prelude::*;
use bevy::math::DVec3;

/// High-precision spatial coordinates using double-precision floating point.
/// This represents the "true" position of an object in the universe.
/// Using DVec3 (f64) allows for much larger coordinate ranges without precision loss.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SpaceCoordinates {
    /// Position in 3D space using double-precision (f64)
    pub position: DVec3,
}

/// Resource defining the center of the rendering coordinate system in Universe space (AU).
/// Used to implement the "floating origin" to avoid f32 jitter at large distances.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct FloatingOrigin {
    pub position: DVec3,
}

/// Resource tracking the currently loaded star system (0 = Sol).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentStarSystem(pub usize);

/// Component identifying which star system a celestial body belongs to.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemId(pub usize);

impl SpaceCoordinates {
    /// Create new space coordinates from a DVec3 position
    pub fn new(position: DVec3) -> Self {
        Self { position }
    }

    /// Create space coordinates from individual x, y, z components
    pub fn from_xyz(x: f64, y: f64, z: f64) -> Self {
        Self {
            position: DVec3::new(x, y, z),
        }
    }
}

/// Keplerian orbital elements for realistic orbital mechanics.
/// All angular measurements are in radians, distances in Astronomical Units (AU).
#[derive(Component, Debug, Clone, Copy)]
pub struct KeplerOrbit {
    /// Eccentricity (e) - shape of the orbit (0 = circle, 0-1 = ellipse, 1 = parabola, >1 = hyperbola)
    pub eccentricity: f64,
    
    /// Semi-major axis (a) - size of the orbit in Astronomical Units (AU)
    pub semi_major_axis: f64,
    
    /// Inclination (i) - tilt of the orbital plane in radians
    pub inclination: f64,
    
    /// Longitude of ascending node (Ω) - where orbit crosses reference plane, in radians
    pub longitude_ascending_node: f64,
    
    /// Argument of periapsis (ω) - orientation of the ellipse in the orbital plane, in radians
    pub argument_of_periapsis: f64,
    
    /// Mean anomaly at epoch (M₀) - position in orbit at time t=0, in radians
    pub mean_anomaly_epoch: f64,
    
    /// Mean motion (n) - radians per second
    /// Derived from orbital period: n = 2π / T
    pub mean_motion: f64,
}

impl KeplerOrbit {
    /// Create a new Keplerian orbit with all parameters
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        eccentricity: f64,
        semi_major_axis: f64,
        inclination: f64,
        longitude_ascending_node: f64,
        argument_of_periapsis: f64,
        mean_anomaly_epoch: f64,
        mean_motion: f64,
    ) -> Self {
        Self {
            eccentricity,
            semi_major_axis,
            inclination,
            longitude_ascending_node,
            argument_of_periapsis,
            mean_anomaly_epoch,
            mean_motion,
        }
    }

    /// Create a circular orbit (eccentricity = 0) at a given radius
    pub fn circular(semi_major_axis: f64, mean_motion: f64) -> Self {
        Self {
            eccentricity: 0.0,
            semi_major_axis,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion,
        }
    }

    /// Calculate the mean motion from orbital period (in seconds)
    /// n = 2π / T
    pub fn mean_motion_from_period(period_seconds: f64) -> f64 {
        if period_seconds > 0.0 {
            std::f64::consts::TAU / period_seconds
        } else {
            0.0
        }
    }

    /// Calculate the orbital period from mean motion
    /// T = 2π / n
    pub fn period_from_mean_motion(mean_motion: f64) -> f64 {
        if mean_motion > 0.0 {
            std::f64::consts::TAU / mean_motion
        } else {
            0.0
        }
    }
}

impl Default for KeplerOrbit {
    fn default() -> Self {
        Self::circular(1.0, 0.0)
    }
}

/// Component that marks an entity as having a visible orbit path
/// Used for orbit visualization
#[derive(Component, Debug, Clone, Copy)]
pub struct OrbitPath {
    /// Color of the orbit line
    pub color: Color,
    
    /// Whether the orbit is currently visible
    pub visible: bool,
    
    /// Number of segments to use when drawing the orbit
    pub segments: u32,
}

impl OrbitPath {
    /// Create a new orbit path with default settings
    pub fn new(color: Color) -> Self {
        Self {
            color,
            visible: true,
            segments: 64,
        }
    }

    /// Create an orbit path with custom segment count
    pub fn with_segments(color: Color, segments: u32) -> Self {
        Self {
            color,
            visible: true,
            segments,
        }
    }
}

impl Default for OrbitPath {
    fn default() -> Self {
        Self::new(Color::srgba(0.5, 0.5, 0.5, 0.3))
    }
}

/// Marker component for selected celestial bodies
/// Selected bodies always have their orbits visible
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Selected;

/// Marker component for hovered celestial bodies
/// Hovered bodies show a glowing ring and name label
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Hovered;

/// Marker component for destroyed/disintegrated celestial bodies.
/// Used for bodies that have been destroyed by natural causes (e.g., ISON solar disintegration),
/// mining operations, weapons, orbital decay, etc.
/// Bodies with this component will be despawned after a brief fade-out period.
#[derive(Component, Debug, Clone, Copy)]
pub struct Destroyed {
    /// Time (in seconds) when the body was destroyed
    pub destruction_time: f64,
    /// Duration (in seconds) of the fade-out animation before despawn
    pub fade_duration: f64,
}

impl Destroyed {
    pub fn new(current_time: f64, fade_duration: f64) -> Self {
        Self {
            destruction_time: current_time,
            fade_duration,
        }
    }

    /// Instant destruction (no fade)
    pub fn instant(current_time: f64) -> Self {
        Self {
            destruction_time: current_time,
            fade_duration: 0.0,
        }
    }
}

/// Marker component for comet tail mesh entities.
/// Used to track and update dynamically generated 3D tail meshes.
#[derive(Component, Debug, Clone, Copy)]
pub struct CometTail {
    /// The entity of the parent comet
    pub comet_entity: Entity,
    /// Whether this is an ion tail (true) or dust tail (false)
    pub is_ion_tail: bool,
}

/// Local orbit amplification factor for moons.
///
/// Scales the orbital position so moons render outside their parent's visual mesh.
/// All moons of the same parent share the same factor to preserve relative spacing.
/// At system-wide zoom levels this is paired with LOD visibility — moons are hidden
/// when the camera is far from the parent, and revealed with amplified orbits when close.
#[derive(Component, Debug, Clone, Copy)]
pub struct LocalOrbitAmplification(pub f32);

/// Marker component for a glossy selection ring mesh.
#[derive(Component, Debug, Clone, Copy)]
pub struct SelectionMarker;

/// Marker component for a glossy hover ring mesh.
#[derive(Component, Debug, Clone, Copy)]
pub struct HoverMarker;

/// Associates a marker entity with its owning celestial body.
#[derive(Component, Debug, Clone, Copy)]
pub struct MarkerOwner(pub Entity);

/// Animated bright dot that moves around a marker ring.
#[derive(Component, Debug, Clone, Copy)]
pub struct MarkerDot {
    pub angle: f32,
    pub angular_speed: f32,
    pub radius: f32,
}

/// Represents a gas component in an atmosphere
#[derive(Debug, Clone, PartialEq)]
pub struct AtmosphericGas {
    /// Name of the gas
    pub name: String,
    /// Percentage of the gas in the atmosphere (0.0 to 100.0)
    pub percentage: f32,
}

impl AtmosphericGas {
    /// Create a new atmospheric gas with a name and percentage
    pub fn new(name: impl Into<String>, percentage: f32) -> Self {
        Self {
            name: name.into(),
            percentage,
        }
    }
}

/// Component representing a celestial body's atmosphere
/// Based on real data from NASA for solar system bodies
#[derive(Component, Debug, Clone)]
pub struct AtmosphereComposition {
    /// Surface pressure in millibars (1 bar = 1000 millibars)
    /// Earth's surface pressure is approximately 1013 millibars
    /// For gas giants, this represents the reference level (conventionally 1 bar)
    pub surface_pressure_mbar: f32,
    
    /// Average surface temperature in Celsius
    pub surface_temperature_celsius: f32,
    
    /// List of atmospheric gases and their percentages
    /// Should sum to approximately 100%
    pub gases: Vec<AtmosphericGas>,
    
    /// Whether the atmosphere is breathable for humans
    /// True if oxygen is present at safe levels (0.1-0.3 atm)
    pub breathable: bool,
    
    /// Whether this body can physically support an atmosphere based on escape velocity.
    /// This uses a simplified binary threshold (≥ 2.0 km/s) for gameplay purposes.
    /// Physically: ≥ 5 km/s retains most gases; 2-5 km/s retains heavy gases; < 2 km/s loses atmosphere.
    pub can_support_atmosphere: bool,
    
    /// Whether this is a reference altitude pressure (true for gas giants) or actual surface pressure (false for terrestrial)
    /// Gas giants lack solid surfaces, so their pressure is measured at the conventional 1 bar reference level
    pub is_reference_pressure: bool,
    
    /// Harvest altitude pressure in bars for gas scooping operations (gas giants only)
    /// This represents the atmospheric pressure level where gas harvesting stations operate.
    /// Deeper = higher pressure = better yield. Default: 10 bar for gas giants, 0 for terrestrial.
    /// Higher values require better technology.
    pub harvest_altitude_bar: f32,
    
    /// Maximum harvest altitude pressure achievable with current technology (gas giants only)
    /// Technology research can increase this limit to allow deeper, more efficient harvesting.
    /// Default: 50 bar for basic tech, can be increased to 100+ bar with advanced tech.
    pub max_harvest_altitude_bar: f32,
}

impl AtmosphereComposition {
    /// Calculate escape velocity in km/s from mass (kg) and radius (km)
    /// Formula: v_e = sqrt(2 * G * M / r)
    /// where G = 6.674e-11 N⋅m²/kg²
    pub fn calculate_escape_velocity(mass_kg: f64, radius_km: f32) -> f64 {
        const G: f64 = 6.674e-11; // Gravitational constant in m³/(kg⋅s²)
        let radius_m = radius_km as f64 * 1000.0; // Convert km to m
        let v_e_m_s = (2.0 * G * mass_kg / radius_m).sqrt();
        v_e_m_s / 1000.0 // Convert m/s to km/s
    }
    
    /// Determine if a body can support an atmosphere based on escape velocity.
    /// 
    /// Returns true if escape velocity ≥ 2.0 km/s (simplified threshold for gameplay).
    /// 
    /// Physical reality (for future enhancement):
    /// - ≥ 5 km/s: Can retain most gases including light gases (H₂, He)
    /// - 2-5 km/s: Can retain heavy gases (N₂, O₂, CO₂) but lose lighter ones over geological time
    /// - < 2 km/s: Cannot retain significant atmospheres over geological timescales
    pub fn can_retain_atmosphere(mass_kg: f64, radius_km: f32) -> bool {
        let escape_velocity = Self::calculate_escape_velocity(mass_kg, radius_km);
        escape_velocity >= 2.0 // Simplified threshold: can retain at least heavy gases
    }
    
    /// Create a new atmosphere composition with mass and radius for calculating retention
    pub fn new_with_body_data(
        surface_pressure_mbar: f32,
        surface_temperature_celsius: f32,
        gases: Vec<AtmosphericGas>,
        body_mass_kg: f64,
        body_radius_km: f32,
        is_reference_pressure: bool,
    ) -> Self {
        // Determine if atmosphere is breathable
        // Need 0.1-0.3 atm of O2 (100-300 mbar)
        let o2_pressure = gases
            .iter()
            .find(|g| g.name == "O2")
            .map(|g| surface_pressure_mbar * g.percentage / 100.0)
            .unwrap_or(0.0);
        
        let breathable = o2_pressure >= 100.0 && o2_pressure <= 300.0;
        
        let can_support_atmosphere = Self::can_retain_atmosphere(body_mass_kg, body_radius_km);
        
        // Set default harvest altitudes for gas giants
        let (harvest_altitude_bar, max_harvest_altitude_bar) = if is_reference_pressure {
            // Gas giants: default 10 bar harvest, max 50 bar with basic tech
            (10.0, 50.0)
        } else {
            // Terrestrial planets: no atmospheric harvesting
            (0.0, 0.0)
        };
        
        Self {
            surface_pressure_mbar,
            surface_temperature_celsius,
            gases,
            breathable,
            can_support_atmosphere,
            is_reference_pressure,
            harvest_altitude_bar,
            max_harvest_altitude_bar,
        }
    }
    
    /// Create a new atmosphere composition (legacy method for backwards compatibility)
    /// Assumes the body can support atmosphere (for compatibility with existing code)
    pub fn new(
        surface_pressure_mbar: f32,
        surface_temperature_celsius: f32,
        gases: Vec<AtmosphericGas>,
    ) -> Self {
        // Determine if atmosphere is breathable
        // Need 0.1-0.3 atm of O2 (100-300 mbar)
        let o2_pressure = gases
            .iter()
            .find(|g| g.name == "O2")
            .map(|g| surface_pressure_mbar * g.percentage / 100.0)
            .unwrap_or(0.0);
        
        let breathable = o2_pressure >= 100.0 && o2_pressure <= 300.0;
        
        Self {
            surface_pressure_mbar,
            surface_temperature_celsius,
            gases,
            breathable,
            can_support_atmosphere: true, // Default to true for backwards compatibility
            is_reference_pressure: false, // Default to surface pressure for backwards compatibility
            harvest_altitude_bar: 0.0,    // No harvesting for terrestrial by default
            max_harvest_altitude_bar: 0.0,
        }
    }
    
    /// Check if the atmosphere has a specific gas
    pub fn has_gas(&self, gas_name: &str) -> bool {
        self.gases.iter().any(|g| g.name == gas_name)
    }
    
    /// Get the percentage of a specific gas
    pub fn get_gas_percentage(&self, gas_name: &str) -> Option<f32> {
        self.gases
            .iter()
            .find(|g| g.name == gas_name)
            .map(|g| g.percentage)
    }
    
    /// Calculate the colony cost based on Aurora 4X model
    /// 0 = Earth-like, 8+ = extremely hostile
    pub fn calculate_colony_cost(&self) -> u8 {
        let mut cost = 0u8;
        
        // Temperature factor
        let temp_diff = (self.surface_temperature_celsius - 15.0).abs();
        if temp_diff > 100.0 {
            cost += 3;
        } else if temp_diff > 50.0 {
            cost += 2;
        } else if temp_diff > 25.0 {
            cost += 1;
        }
        
        // Pressure factor
        let pressure_bar = self.surface_pressure_mbar / 1000.0;
        if pressure_bar < 0.01 || pressure_bar > 10.0 {
            cost += 3;
        } else if pressure_bar < 0.5 || pressure_bar > 2.0 {
            cost += 2;
        } else if pressure_bar < 0.8 || pressure_bar > 1.5 {
            cost += 1;
        }
        
        // Breathability factor
        if !self.breathable {
            cost += 2;
        }
        
        cost.min(8)
    }
    
    /// Calculate harvest yield multiplier based on harvest altitude vs reference pressure.
    /// For gas giants, deeper atmospheric harvesting yields more gas per volume.
    /// Uses simplified ideal gas law approximation: density ∝ pressure at constant temperature.
    /// 
    /// Returns multiplier relative to 1 bar reference level:
    /// - At 1 bar: 1.0x yield
    /// - At 10 bar: ~10x yield
    /// - At 50 bar: ~50x yield
    pub fn harvest_yield_multiplier(&self) -> f32 {
        if !self.is_reference_pressure {
            // Terrestrial planets: no atmospheric harvesting
            return 0.0;
        }
        
        // For gas giants, yield is proportional to pressure/density
        // Using harvest altitude relative to 1 bar reference
        let reference_bar = self.surface_pressure_mbar / 1000.0;
        if reference_bar <= 0.0 || self.harvest_altitude_bar <= 0.0 {
            return 0.0;
        }
        
        // Yield multiplier is approximately harvest pressure / reference pressure
        self.harvest_altitude_bar / reference_bar
    }
    
    /// Check if harvest altitude can be increased (not at maximum yet)
    pub fn can_increase_harvest_altitude(&self) -> bool {
        self.is_reference_pressure && self.harvest_altitude_bar < self.max_harvest_altitude_bar
    }
    
    /// Get remaining harvest altitude capacity (how much deeper we can go with tech upgrades)
    pub fn remaining_harvest_capacity_bar(&self) -> f32 {
        if !self.is_reference_pressure {
            return 0.0;
        }
        (self.max_harvest_altitude_bar - self.harvest_altitude_bar).max(0.0)
    }
}
