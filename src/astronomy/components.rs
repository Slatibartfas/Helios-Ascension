use bevy::math::DVec3;
use bevy::prelude::*;

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

/// Component referencing the entity that this body orbits around.
/// Without this component, orbits are computed relative to the universe origin (0,0,0).
/// With it, the orbit position is offset by the parent entity's SpaceCoordinates.
#[derive(Component, Debug, Clone, Copy)]
pub struct OrbitCenter(pub Entity);

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

// ── Lagrange-point hover / selection ─────────────────────────────────────────

/// Per-frame data for one rendered Lagrange-point marker.
/// Stored in [`LagrangePointMarkers`] so hover / selection systems can reference
/// LP positions without needing the full rendering system.
#[derive(Debug, Clone)]
pub struct LpMarkerInfo {
    /// World-space render position of this marker.
    pub render_pos: bevy::math::Vec3,
    /// Hit-test radius in render units (matches the drawn dot/circle size).
    pub hit_radius: f32,
    /// L-point index: 1 = L1, 2 = L2, 3 = L3, 4 = L4, 5 = L5.
    pub point: u8,
    /// ECS entity of the parent planet whose L-points these are.
    pub planet_entity: bevy::ecs::entity::Entity,
    /// Human-readable planet name (e.g. "Earth").
    pub planet_name: String,
    /// Planet's heliocentric semi-major axis in AU.
    pub planet_sma_au: f64,
    /// Effective heliocentric orbital radius of this LP in AU.
    pub lp_radius_au: f64,
    /// Gravitational parameter of the central star (m³ s⁻²).
    pub gm: f64,
}

/// Resource populated each frame by [`draw_lagrange_point_rings`].
///
/// Cleared at the start of the system and re-filled with the current frame's
/// LP marker positions.  Used by hover-detection and selection systems that
/// run after rendering.
#[derive(Resource, Default)]
pub struct LagrangePointMarkers {
    /// All LP markers drawn this frame.
    pub markers: Vec<LpMarkerInfo>,
    /// Index into `markers` of the currently hovered LP, if any.
    pub hovered_index: Option<usize>,
}

/// Resource set by [`handle_lp_hover`] when the player left-clicks on a
/// Lagrange-point marker.  UI systems consume & clear this each frame.
#[derive(Resource, Default)]
pub struct LastLpClick {
    pub info: Option<LpMarkerInfo>,
}

/// The type of liquid ocean on a celestial body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum OceanType {
    /// Liquid water (Earth, potentially Mars in the past)
    Water,
    /// Liquid methane/ethane (Titan)
    Methane,
    /// Liquid ammonia (hypothetical super-Earths)
    Ammonia,
    /// Hydrocarbon mix — methane + ethane lakes (Titan)
    Hydrocarbon,
    /// Subsurface ocean beneath ice crust (Europa, Enceladus, Ganymede)
    Subsurface,
}

/// Component describing a body's ocean properties.
///
/// Attached to any body that has a significant liquid surface or subsurface
/// ocean. Used for visuals (ocean shell), resource phase logic, and colony
/// habitability modifiers.
#[derive(Component, Debug, Clone, Copy)]
pub struct OceanProperties {
    /// What liquid the ocean is made of.
    pub ocean_type: OceanType,
    /// Fraction of the body's surface covered by liquid (0.0–1.0).
    /// For subsurface oceans this is typically 1.0 (global ocean under ice).
    pub surface_fraction: f32,
    /// Average depth of the ocean in km.
    pub mean_depth_km: f32,
    /// Whether the ocean is beneath an ice shell (Europa-style).
    pub is_subsurface: bool,
}

impl OceanProperties {
    /// Colony habitability multiplier based on ocean presence.
    /// Water oceans boost growth; exotic liquids are neutral or slightly negative.
    pub fn habitability_modifier(&self) -> f64 {
        if self.is_subsurface {
            return 1.0; // Subsurface oceans don't directly help surface colonies
        }
        match self.ocean_type {
            OceanType::Water => 1.0 + (self.surface_fraction as f64) * 0.5, // Up to +50%
            OceanType::Ammonia => 0.9,  // Mildly hostile
            OceanType::Methane | OceanType::Hydrocarbon => 0.85, // Hostile
            OceanType::Subsurface => 1.0,
        }
    }
}

/// Infer ocean properties from temperature, pressure and atmosphere composition.
///
/// Called during procedural generation to decide whether a body should have
/// surface or subsurface liquids based on physical conditions.
pub fn infer_ocean_properties(
    avg_temp_c: f32,
    surface_pressure_mbar: f32,
    has_water_deposits: bool,
    has_methane: bool,
    radius_km: f32,
) -> Option<OceanProperties> {
    // Water ocean: requires liquid-water temperature range and sufficient pressure
    if has_water_deposits && avg_temp_c > 0.0 && avg_temp_c < 100.0 && surface_pressure_mbar > 6.1
    {
        let fraction = if avg_temp_c > 10.0 && avg_temp_c < 50.0 {
            0.6 // Temperate → large ocean coverage
        } else {
            0.3 // Edge of habitability → smaller coverage
        };
        return Some(OceanProperties {
            ocean_type: OceanType::Water,
            surface_fraction: fraction,
            mean_depth_km: 3.0, // Earth-like average
            is_subsurface: false,
        });
    }

    // Methane/hydrocarbon lakes (Titan-like): very cold, thick atmosphere with CH4
    if has_methane
        && avg_temp_c > -183.0
        && avg_temp_c < -161.0
        && surface_pressure_mbar > 100.0
    {
        return Some(OceanProperties {
            ocean_type: OceanType::Hydrocarbon,
            surface_fraction: 0.02, // Titan has ~1.6% lake coverage
            mean_depth_km: 0.15,
            is_subsurface: false,
        });
    }

    // Subsurface ocean: icy moon heuristic — small, cold body with water ice
    if has_water_deposits && avg_temp_c < -20.0 && radius_km > 200.0 && radius_km < 3000.0 {
        return Some(OceanProperties {
            ocean_type: OceanType::Subsurface,
            surface_fraction: 1.0,
            mean_depth_km: 50.0,
            is_subsurface: true,
        });
    }

    None
}

/// Component for the surface temperature of a celestial body.
/// This exists for all solid bodies, regardless of whether they have an atmosphere.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SurfaceTemperature {
    pub average_celsius: f32,
    pub min_celsius: f32,
    pub max_celsius: f32,
}

/// Component storing stellar properties for stars.
/// Used for calculating illumination, temperature, and habitability of orbiting bodies.
#[derive(Component, Debug, Clone, Copy)]
pub struct StellarProperties {
    /// Luminosity relative to Sol (L☉)
    /// Sol = 1.0
    pub luminosity_sol: f32,
    /// Effective surface temperature in Kelvin
    pub temperature_kelvin: f32,
}

impl StellarProperties {
    /// Create new stellar properties
    pub fn new(luminosity_sol: f32, temperature_kelvin: f32) -> Self {
        Self {
            luminosity_sol,
            temperature_kelvin,
        }
    }

    /// Create stellar properties for Sol
    pub fn sol() -> Self {
        Self {
            luminosity_sol: 1.0,
            temperature_kelvin: 5778.0,
        }
    }
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

    // --- Derived / cached scattering parameters ---

    /// Scale height in km (how quickly density drops with altitude).
    pub scale_height_km: f32,

    /// Rayleigh scattering colour tint (RGB, normalised).
    pub rayleigh_rgb: [f32; 3],

    /// Rayleigh scattering strength multiplier.
    pub rayleigh_strength: f32,

    /// Mie (aerosol/haze) scattering strength multiplier.
    pub mie_strength: f32,

    /// Mie asymmetry parameter g (0.0 = isotropic, ~0.76 = forward-scattering typical).
    pub mie_g: f32,

    /// Haze / aerosol colour (RGB, normalised).
    pub haze_color: [f32; 3],

    /// Overall intensity multiplier for the atmosphere visual.
    pub atmosphere_intensity: f32,
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
            // Scattering defaults — will be overridden by set_scattering_params()
            scale_height_km: 8.5,
            rayleigh_rgb: [0.175, 0.41, 1.0],
            rayleigh_strength: 1.0,
            mie_strength: 0.005,
            mie_g: 0.76,
            haze_color: [1.0, 0.9, 0.7],
            atmosphere_intensity: 1.0,
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
            // Scattering defaults
            scale_height_km: 8.5,
            rayleigh_rgb: [0.175, 0.41, 1.0],
            rayleigh_strength: 1.0,
            mie_strength: 0.005,
            mie_g: 0.76,
            haze_color: [1.0, 0.9, 0.7],
            atmosphere_intensity: 1.0,
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

    // ── Atmospheric scattering helpers ──────────────────────────────────

    /// Compute the mean molecular weight of the atmosphere in g/mol,
    /// based on gas composition percentages.
    pub fn mean_molecular_weight(&self) -> f32 {
        let mut total = 0.0_f32;
        for gas in &self.gases {
            let mw = match gas.name.as_str() {
                "H2" => 2.016,
                "He" => 4.003,
                "CH4" => 16.04,
                "NH3" => 17.03,
                "H2O" => 18.015,
                "Ne" => 20.18,
                "N2" => 28.014,
                "CO" => 28.01,
                "O2" => 31.998,
                "H2S" => 34.08,
                "Ar" => 39.948,
                "CO2" => 44.01,
                "SO2" => 64.066,
                _ => 28.97, // default to air-like
            };
            total += mw * gas.percentage / 100.0;
        }
        if total <= 0.0 { 28.97 } else { total }
    }

    /// Derive and set all scattering parameters from physical properties.
    ///
    /// Uses surface pressure, temperature, gas composition and body gravity
    /// to compute plausible Rayleigh/Mie parameters.  RON overrides (passed
    /// via the `AtmosphereData` optional fields) take precedence.
    pub fn derive_scattering_params(
        &mut self,
        surface_gravity_g: f32,
        override_scale_height: Option<f32>,
        override_rayleigh_rgb: Option<(f32, f32, f32)>,
        override_rayleigh_strength: Option<f32>,
        override_mie_strength: Option<f32>,
        override_mie_g: Option<f32>,
        override_haze_color: Option<(f32, f32, f32)>,
        override_intensity: Option<f32>,
    ) {
        // 1. Scale height: H = kT / (m g)
        //    Using ratio to Earth: H = 8.5 * (T/288) * (1/g_ratio) * (28.97/mmw)
        let t_kelvin = self.surface_temperature_celsius + 273.15;
        let mmw = self.mean_molecular_weight();
        let gravity = surface_gravity_g.max(0.01);
        let scale_height = override_scale_height.unwrap_or_else(|| {
            8.5 * (t_kelvin / 288.15) * (1.0 / gravity) * (28.97 / mmw)
        });
        self.scale_height_km = scale_height.clamp(1.0, 1000.0);

        // 2. Rayleigh RGB tint — choose base colour from dominant composition
        let base_rayleigh = override_rayleigh_rgb.map(|c| [c.0, c.1, c.2]).unwrap_or_else(|| {
            // CO2 dominant → warm red-orange sky
            if self.get_gas_percentage("CO2").unwrap_or(0.0) > 50.0 {
                [1.0, 0.5, 0.2]
            }
            // H2/He dominant (ice & gas giants) → blue/cyan; checked BEFORE CH4
            // so Uranus (H2 83%, CH4 2%) and Neptune (H2 80%, CH4 1.5%) are not
            // misclassified as Titan-like orange.
            else if self.get_gas_percentage("H2").unwrap_or(0.0) > 50.0 {
                [0.35, 0.55, 1.0]
            }
            // CH4 rich in a non-H2 atmosphere (Titan-like) → blue-muted
            else if self.get_gas_percentage("CH4").unwrap_or(0.0) > 1.0 {
                [0.3, 0.5, 0.9]
            }
            // N2/O2 dominant (Earth-like) → classic blue
            else {
                [0.175, 0.41, 1.0]
            }
        });
        self.rayleigh_rgb = base_rayleigh;

        // 3. Rayleigh strength — proportional to pressure / Earth reference
        let pressure_ratio = self.surface_pressure_mbar / 1013.25;
        self.rayleigh_strength = override_rayleigh_strength
            .unwrap_or_else(|| (pressure_ratio * (self.scale_height_km / 8.5)).clamp(0.0, 50.0));

        // 4. Mie / haze
        let haze_factor = if self.get_gas_percentage("CH4").unwrap_or(0.0) > 1.0 {
            0.08 // Titan-like thick haze
        } else if self.get_gas_percentage("CO2").unwrap_or(0.0) > 50.0 {
            0.03 // Mars/Venus dust
        } else {
            0.005 // Earth-like clean air
        };
        self.mie_strength = override_mie_strength
            .unwrap_or_else(|| (pressure_ratio.sqrt() * haze_factor).clamp(0.0, 1.0));
        self.mie_g = override_mie_g.unwrap_or(0.76);

        // 5. Haze colour
        self.haze_color = override_haze_color.map(|c| [c.0, c.1, c.2]).unwrap_or_else(|| {
            let h2_pct  = self.get_gas_percentage("H2").unwrap_or(0.0);
            let co2_pct = self.get_gas_percentage("CO2").unwrap_or(0.0);
            let ch4_pct = self.get_gas_percentage("CH4").unwrap_or(0.0);
            if co2_pct > 50.0 {
                [1.0, 0.6, 0.3]   // warm brown/orange (Mars, Venus)
            } else if h2_pct > 50.0 {
                [0.85, 0.92, 1.0]  // pale blue-white (H2/He ice & gas giants — Uranus, Neptune, etc.)
            } else if ch4_pct > 1.0 {
                [1.0, 0.7, 0.3]   // amber/orange (N2+CH4 atmosphere — Titan organics)
            } else {
                [1.0, 0.95, 0.88] // near-white (Earth clean aerosol)
            }
        });

        // 6. Overall intensity
        self.atmosphere_intensity = override_intensity.unwrap_or(1.0);
    }

    /// Calculate the colony cost.
    /// Returns the colony cost factor (0.0 = Earth-like/Ideal).
    /// Returns f32::INFINITY if the body is uninhabitable for standard humans (e.g. extreme gravity).
    pub fn calculate_colony_cost(&self, gravity_g: f32, min_temp_c: f32, max_temp_c: f32) -> f32 {
        calculate_general_colony_cost(gravity_g, min_temp_c, max_temp_c, Some(self), self.is_reference_pressure)
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

/// Detailed breakdown of colony cost factors
#[derive(Debug, Clone, Copy, Default)]
pub struct ColonyCostDetails {
    pub total_cost: f32,
    pub base_cost: f32,
    pub heavy_gravity_limit_exceeded: bool,
    pub is_gas_giant: bool,
    pub heat_cost: f32,
    pub cold_cost: f32,
    pub pressure_cost: f32,
    pub low_gravity_penalty: f32,
}

/// Calculate detailed colony cost breakdown
pub fn calculate_colony_cost_details(
    gravity_g: f32,
    min_temp_c: f32,
    max_temp_c: f32,
    atmosphere: Option<&AtmosphereComposition>,
    is_gas_giant: bool,
) -> ColonyCostDetails {
    // Standard Human Tolerances
    const MIN_GRAVITY: f32 = 0.1;
    const MAX_GRAVITY: f32 = 1.7;
    const MIN_BREATHABLE_TEMP: f32 = 0.0;
    const MAX_BREATHABLE_TEMP: f32 = 40.0;

    let mut details = ColonyCostDetails::default();
    
    // 0. Gas Giant Check (Hard Limit - no solid surface)
    if is_gas_giant {
        details.is_gas_giant = true;
        details.total_cost = f32::INFINITY;
        return details;
    }

    // 1. Gravity Check (Hard Limit)
    if gravity_g > MAX_GRAVITY {
        details.heavy_gravity_limit_exceeded = true;
        details.total_cost = f32::INFINITY;
        return details;
    }

    // 2. Base Infrastructure Cost
    // If no atmosphere or not breathable, base cost is 2.0 (Closed Cycle/Pressurized)
    let breathable = atmosphere.map_or(false, |a| a.breathable);
    if !breathable {
        details.base_cost = 2.0;
        details.total_cost += 2.0;
    }

    // 3. Temperature Cost
    // Deviation below minimum (Heating required)
    if min_temp_c < MIN_BREATHABLE_TEMP {
        details.cold_cost = (MIN_BREATHABLE_TEMP - min_temp_c).abs() / 10.0;
        details.total_cost += details.cold_cost;
    }

    // Deviation above maximum (Cooling required)
    if max_temp_c > MAX_BREATHABLE_TEMP {
        details.heat_cost = (max_temp_c - MAX_BREATHABLE_TEMP).abs() / 10.0;
        details.total_cost += details.heat_cost;
    }

    // 4. Pressure Cost (only if atmosphere exists)
    if let Some(atm) = atmosphere {
        let pressure_bar = atm.surface_pressure_mbar / 1000.0;
        if pressure_bar > 4.0 {
             // High pressure penalty
             details.pressure_cost = (pressure_bar - 4.0) * 0.5;
             details.total_cost += details.pressure_cost;
        }
    }

    // 5. Low Gravity Penalty
    if gravity_g < MIN_GRAVITY {
        details.low_gravity_penalty = 1.0;
        details.total_cost += 1.0;
    }

    details
}

/// Calculate colony cost for any body, even without atmosphere.
///
/// Returns the colony cost factor (0.0 = Earth-like/Ideal).
/// Returns f32::INFINITY if the body is uninhabitable for standard humans (e.g. extreme gravity).
pub fn calculate_general_colony_cost(gravity_g: f32, min_temp_c: f32, max_temp_c: f32, atmosphere: Option<&AtmosphereComposition>, is_gas_giant: bool) -> f32 {
    let details = calculate_colony_cost_details(gravity_g, min_temp_c, max_temp_c, atmosphere, is_gas_giant);
    details.total_cost
}
