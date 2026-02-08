//! Exoplanet data structures and integration with NASA Exoplanet Archive
//! 
//! This module provides structures to hold confirmed exoplanet data and
//! integrate it with the procedural generation system.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents a confirmed exoplanet from the NASA Exoplanet Archive
/// These are spawned as 'Real' planets before procedural gap-filling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmedPlanet {
    /// Name of the planet
    pub name: String,
    
    /// Mass of the planet in Earth masses (M⊕)
    /// None if mass is unknown
    pub mass_earth: Option<f32>,
    
    /// Radius of the planet in Earth radii (R⊕)
    /// None if radius is unknown
    pub radius_earth: Option<f32>,
    
    /// Orbital period in days
    pub period_days: f32,
    
    /// Semi-major axis of the orbit in Astronomical Units (AU)
    pub semi_major_axis_au: f64,
    
    /// Orbital eccentricity (0 = circular, <1 = elliptical)
    pub eccentricity: f64,
    
    /// Orbital inclination in degrees
    /// None if inclination is unknown
    pub inclination_deg: Option<f64>,
    
    /// Planet type classification (e.g., "Telluric", "Gas Giant", "Ice Giant", "Super-Earth")
    #[serde(rename = "type")]
    pub planet_type: String,
    
    /// Discovery method (e.g., "Transit", "Radial Velocity", "Direct Imaging")
    #[serde(default)]
    pub discovery_method: Option<String>,
    
    /// Discovery year
    #[serde(default)]
    pub discovery_year: Option<u16>,
    
    /// Equilibrium temperature in Kelvin
    /// Calculated from stellar flux and assuming zero albedo
    #[serde(default)]
    pub equilibrium_temp_k: Option<f32>,
}

impl ConfirmedPlanet {
    /// Create a new confirmed planet with required parameters
    pub fn new(
        name: String,
        semi_major_axis_au: f64,
        period_days: f32,
        eccentricity: f64,
        planet_type: String,
    ) -> Self {
        Self {
            name,
            mass_earth: None,
            radius_earth: None,
            period_days,
            semi_major_axis_au,
            eccentricity,
            inclination_deg: None,
            planet_type,
            discovery_method: None,
            discovery_year: None,
            equilibrium_temp_k: None,
        }
    }
    
    /// Set the mass of the planet in Earth masses
    pub fn with_mass(mut self, mass_earth: f32) -> Self {
        self.mass_earth = Some(mass_earth);
        self
    }
    
    /// Set the radius of the planet in Earth radii
    pub fn with_radius(mut self, radius_earth: f32) -> Self {
        self.radius_earth = Some(radius_earth);
        self
    }
    
    /// Set the orbital inclination in degrees
    pub fn with_inclination(mut self, inclination_deg: f64) -> Self {
        self.inclination_deg = Some(inclination_deg);
        self
    }
    
    /// Set the discovery method
    pub fn with_discovery_method(mut self, method: String) -> Self {
        self.discovery_method = Some(method);
        self
    }
    
    /// Set the discovery year
    pub fn with_discovery_year(mut self, year: u16) -> Self {
        self.discovery_year = Some(year);
        self
    }
    
    /// Set the equilibrium temperature
    pub fn with_equilibrium_temp(mut self, temp_k: f32) -> Self {
        self.equilibrium_temp_k = Some(temp_k);
        self
    }
    
    /// Estimate mass from radius using mass-radius relationships if mass is unknown
    /// Based on empirical relationships for different planet types
    pub fn estimated_mass_earth(&self) -> f32 {
        if let Some(mass) = self.mass_earth {
            return mass;
        }
        
        if let Some(radius) = self.radius_earth {
            // Use empirical mass-radius relationships
            // These are approximations based on planetary composition
            match self.planet_type.as_str() {
                "Telluric" | "Rocky" | "Super-Earth" => {
                    // Rocky planet: M ∝ R^3.7 (Chen & Kipping 2017)
                    radius.powf(3.7)
                }
                "Gas Giant" | "Jupiter-like" => {
                    // Gas giant: M ∝ R^1.0 (roughly constant density for Jovian planets)
                    radius
                }
                "Ice Giant" | "Neptune-like" => {
                    // Ice giant: M ∝ R^2.5
                    radius.powf(2.5)
                }
                _ => {
                    // Generic fallback: M ∝ R^3.0 (constant density)
                    radius.powf(3.0)
                }
            }
        } else {
            // If both mass and radius are unknown, use a default based on type
            match self.planet_type.as_str() {
                "Telluric" | "Rocky" => 1.0,
                "Super-Earth" => 3.0,
                "Neptune-like" | "Ice Giant" => 15.0,
                "Gas Giant" | "Jupiter-like" => 300.0,
                _ => 1.0,
            }
        }
    }
    
    /// Estimate radius from mass using mass-radius relationships if radius is unknown
    pub fn estimated_radius_earth(&self) -> f32 {
        if let Some(radius) = self.radius_earth {
            return radius;
        }
        
        if let Some(mass) = self.mass_earth {
            // Inverse of mass-radius relationships
            match self.planet_type.as_str() {
                "Telluric" | "Rocky" | "Super-Earth" => {
                    // R ∝ M^(1/3.7)
                    mass.powf(1.0 / 3.7)
                }
                "Gas Giant" | "Jupiter-like" => {
                    // R ∝ M^1.0
                    mass
                }
                "Ice Giant" | "Neptune-like" => {
                    // R ∝ M^(1/2.5)
                    mass.powf(1.0 / 2.5)
                }
                _ => {
                    // Generic fallback: R ∝ M^(1/3)
                    mass.powf(1.0 / 3.0)
                }
            }
        } else {
            // If both mass and radius are unknown, use a default based on type
            match self.planet_type.as_str() {
                "Telluric" | "Rocky" => 1.0,
                "Super-Earth" => 1.5,
                "Neptune-like" | "Ice Giant" => 4.0,
                "Gas Giant" | "Jupiter-like" => 11.0,
                _ => 1.0,
            }
        }
    }
    
    /// Calculate mean motion from orbital period
    /// n = 2π / T (where T is in seconds)
    pub fn mean_motion(&self) -> f64 {
        let period_seconds = (self.period_days as f64) * 86400.0; // days to seconds
        std::f64::consts::TAU / period_seconds
    }
}

/// Marker component indicating this planet is a confirmed exoplanet (real data)
/// as opposed to a procedurally generated planet
#[derive(Component, Debug, Clone, Copy)]
pub struct RealPlanet;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirmed_planet_creation() {
        let planet = ConfirmedPlanet::new(
            "Proxima Centauri b".to_string(),
            0.0485,
            11.186,
            0.11,
            "Telluric".to_string(),
        )
        .with_mass(1.17)
        .with_radius(1.1);
        
        assert_eq!(planet.name, "Proxima Centauri b");
        assert_eq!(planet.semi_major_axis_au, 0.0485);
        assert_eq!(planet.mass_earth, Some(1.17));
        assert_eq!(planet.radius_earth, Some(1.1));
    }
    
    #[test]
    fn test_mass_estimation_from_radius() {
        let rocky = ConfirmedPlanet::new(
            "Test Rocky".to_string(),
            1.0,
            365.0,
            0.0,
            "Telluric".to_string(),
        )
        .with_radius(1.5);
        
        let mass = rocky.estimated_mass_earth();
        // For radius 1.5, M ∝ R^3.7 = 1.5^3.7 ≈ 2.95
        assert!(mass > 2.5 && mass < 3.5);
    }
    
    #[test]
    fn test_radius_estimation_from_mass() {
        let gas_giant = ConfirmedPlanet::new(
            "Test Gas Giant".to_string(),
            5.0,
            4300.0,
            0.05,
            "Gas Giant".to_string(),
        )
        .with_mass(318.0); // Jupiter mass
        
        let radius = gas_giant.estimated_radius_earth();
        // For gas giants, R ≈ M (roughly)
        assert!(radius > 300.0 && radius < 320.0);
    }
    
    #[test]
    fn test_mean_motion_calculation() {
        let earth_like = ConfirmedPlanet::new(
            "Test Earth".to_string(),
            1.0,
            365.25,
            0.0167,
            "Telluric".to_string(),
        );
        
        let mean_motion = earth_like.mean_motion();
        // For 365.25 days, mean motion should be close to Earth's
        // n = 2π / (365.25 * 86400) ≈ 1.991e-7 rad/s
        assert!((mean_motion - 1.991e-7).abs() < 1e-9);
    }
}
