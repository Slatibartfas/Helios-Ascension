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
