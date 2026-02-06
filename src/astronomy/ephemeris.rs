//! Ephemeris calculations for celestial body positions
//!
//! This module calculates orbital positions (mean anomalies) for celestial bodies
//! based on a given date, using J2000.0 epoch orbital elements.
//!
//! J2000.0 epoch: January 1, 2000, 12:00 TT (Terrestrial Time)
//!
//! # Example Usage
//!
//! Calculate positions for all bodies at a specific date:
//! ```rust,ignore
//! use helios_ascension::astronomy::calculate_positions_at_timestamp;
//!
//! // January 1, 2026 00:00 UTC
//! let timestamp = 1_767_225_600;
//! let positions = calculate_positions_at_timestamp(timestamp);
//!
//! // Get mean anomaly for Earth (in degrees)
//! let earth_position = positions.get("Earth").unwrap();
//! println!("Earth mean anomaly: {:.2}°", earth_position);
//! ```
//!
//! For custom game start dates:
//! ```rust,ignore
//! use helios_ascension::astronomy::calculate_positions_at_timestamp;
//! use helios_ascension::ui::SimulationTime;
//!
//! // Create simulation time with custom start date
//! let custom_start = 1_893_456_000; // Some future date
//! let sim_time = SimulationTime::with_start_timestamp(custom_start);
//!
//! // Calculate orbital positions for that date
//! let positions = calculate_positions_at_timestamp(custom_start);
//!
//! // Use positions.get("Mercury"), positions.get("Venus"), etc.
//! // to set initial_angle in CelestialBodyData before spawning bodies
//! ```

use std::collections::HashMap;

/// Number of days from Unix epoch to J2000.0 epoch
/// J2000.0 is January 1, 2000, 12:00 TT
/// Unix epoch is January 1, 1970, 00:00 UTC
/// J2000.0 is 10957.5 days after Unix epoch (30 years * 365.25 + 0.5 day)
const DAYS_UNIX_EPOCH_TO_J2000: f64 = 10957.5;

/// J2000 orbital elements for a celestial body
#[derive(Debug, Clone)]
pub struct J2000Elements {
    /// Orbital period in Earth days
    pub period: f64,
    /// Mean longitude at J2000 epoch (degrees)
    pub mean_longitude_j2000: f64,
    /// Longitude of perihelion at J2000 epoch (degrees)
    pub longitude_perihelion: f64,
}

impl J2000Elements {
    /// Calculate mean anomaly for a given number of days from J2000 epoch
    pub fn mean_anomaly_at_days(&self, days_from_j2000: f64) -> f64 {
        // Mean motion (degrees per day)
        let n = 360.0 / self.period;
        
        // Mean longitude at target date
        let mean_longitude = self.mean_longitude_j2000 + n * days_from_j2000;
        
        // Mean anomaly = Mean longitude - Longitude of perihelion
        let mut mean_anomaly = mean_longitude - self.longitude_perihelion;
        
        // Normalize to 0-360 range
        mean_anomaly = mean_anomaly % 360.0;
        if mean_anomaly < 0.0 {
            mean_anomaly += 360.0;
        }
        
        mean_anomaly
    }
}

/// Simplified orbital elements for moons and other bodies
/// Uses a base offset for semi-random but consistent distribution
#[derive(Debug, Clone)]
pub struct SimpleElements {
    /// Orbital period in Earth days
    pub period: f64,
    /// Base offset for visual distribution (degrees)
    pub base_offset: f64,
}

impl SimpleElements {
    /// Calculate mean anomaly for a given number of days from reference epoch
    pub fn mean_anomaly_at_days(&self, days_from_reference: f64) -> f64 {
        // Mean motion (degrees per day)
        let n = 360.0 / self.period;
        
        // Mean anomaly from base offset
        let mut mean_anomaly = self.base_offset + n * days_from_reference;
        
        // Normalize to 0-360 range
        mean_anomaly = mean_anomaly % 360.0;
        if mean_anomaly < 0.0 {
            mean_anomaly += 360.0;
        }
        
        mean_anomaly
    }
}

/// Get J2000 orbital elements for major planets
/// Source: NASA JPL approximations for J2000
pub fn planet_elements() -> HashMap<String, J2000Elements> {
    let mut elements = HashMap::new();
    
    elements.insert("Mercury".to_string(), J2000Elements {
        period: 87.969,
        mean_longitude_j2000: 252.25084,
        longitude_perihelion: 77.45645,
    });
    
    elements.insert("Venus".to_string(), J2000Elements {
        period: 224.701,
        mean_longitude_j2000: 181.97973,
        longitude_perihelion: 131.53298,
    });
    
    elements.insert("Earth".to_string(), J2000Elements {
        period: 365.256363,
        mean_longitude_j2000: 100.46435,
        longitude_perihelion: 102.94719,
    });
    
    elements.insert("Mars".to_string(), J2000Elements {
        period: 686.980,
        mean_longitude_j2000: 355.45332,
        longitude_perihelion: 336.04084,
    });
    
    elements.insert("Jupiter".to_string(), J2000Elements {
        period: 4332.589,
        mean_longitude_j2000: 34.40438,
        longitude_perihelion: 14.75385,
    });
    
    elements.insert("Saturn".to_string(), J2000Elements {
        period: 10759.22,
        mean_longitude_j2000: 49.94432,
        longitude_perihelion: 92.43194,
    });
    
    elements.insert("Uranus".to_string(), J2000Elements {
        period: 30685.4,
        mean_longitude_j2000: 313.23218,
        longitude_perihelion: 170.96424,
    });
    
    elements.insert("Neptune".to_string(), J2000Elements {
        period: 60189.0,
        mean_longitude_j2000: 304.88003,
        longitude_perihelion: 44.97135,
    });
    
    elements
}

/// Get simplified orbital elements for major moons
/// Uses distributed base offsets for visual variety
pub fn moon_elements() -> HashMap<String, SimpleElements> {
    let mut elements = HashMap::new();
    
    // Moon (Earth's moon)
    elements.insert("Moon".to_string(), SimpleElements {
        period: 27.321661,
        base_offset: 135.0,
    });
    
    // Mars moons
    elements.insert("Phobos".to_string(), SimpleElements {
        period: 0.31891023,
        base_offset: 30.0,
    });
    
    elements.insert("Deimos".to_string(), SimpleElements {
        period: 1.263,
        base_offset: 150.0,
    });
    
    // Galilean moons (Jupiter)
    elements.insert("Io".to_string(), SimpleElements {
        period: 1.769138,
        base_offset: 45.0,
    });
    
    elements.insert("Europa".to_string(), SimpleElements {
        period: 3.551181,
        base_offset: 135.0,
    });
    
    elements.insert("Ganymede".to_string(), SimpleElements {
        period: 7.154553,
        base_offset: 225.0,
    });
    
    elements.insert("Callisto".to_string(), SimpleElements {
        period: 16.689018,
        base_offset: 315.0,
    });
    
    // Saturn moons
    elements.insert("Titan".to_string(), SimpleElements {
        period: 15.945,
        base_offset: 90.0,
    });
    
    elements.insert("Rhea".to_string(), SimpleElements {
        period: 4.518212,
        base_offset: 180.0,
    });
    
    elements.insert("Iapetus".to_string(), SimpleElements {
        period: 79.3215,
        base_offset: 270.0,
    });
    
    // Uranus moons
    elements.insert("Titania".to_string(), SimpleElements {
        period: 8.706234,
        base_offset: 60.0,
    });
    
    elements.insert("Oberon".to_string(), SimpleElements {
        period: 13.463234,
        base_offset: 210.0,
    });
    
    // Neptune moons
    elements.insert("Triton".to_string(), SimpleElements {
        period: 5.876854,
        base_offset: 120.0,
    });
    
    elements
}

/// Get simplified orbital elements for dwarf planets
pub fn dwarf_planet_elements() -> HashMap<String, SimpleElements> {
    let mut elements = HashMap::new();
    
    elements.insert("Pluto".to_string(), SimpleElements {
        period: 90560.0,
        base_offset: 180.0,
    });
    
    elements.insert("Ceres".to_string(), SimpleElements {
        period: 1682.0,
        base_offset: 45.0,
    });
    
    elements.insert("Eris".to_string(), SimpleElements {
        period: 203670.0,
        base_offset: 270.0,
    });
    
    elements.insert("Makemake".to_string(), SimpleElements {
        period: 111845.0,
        base_offset: 90.0,
    });
    
    elements.insert("Haumea".to_string(), SimpleElements {
        period: 103468.0,
        base_offset: 135.0,
    });
    
    elements
}

/// Calculate mean anomalies for all celestial bodies at a given Unix timestamp
pub fn calculate_positions_at_timestamp(unix_timestamp: i64) -> HashMap<String, f64> {
    let mut positions = HashMap::new();
    
    // Convert Unix timestamp to days from J2000
    let days_from_unix_epoch = unix_timestamp as f64 / 86400.0;
    let days_from_j2000 = days_from_unix_epoch - DAYS_UNIX_EPOCH_TO_J2000;
    
    // Calculate planetary positions
    for (name, elements) in planet_elements() {
        let mean_anomaly = elements.mean_anomaly_at_days(days_from_j2000);
        positions.insert(name, mean_anomaly);
    }
    
    // Calculate moon positions
    for (name, elements) in moon_elements() {
        let mean_anomaly = elements.mean_anomaly_at_days(days_from_j2000);
        positions.insert(name, mean_anomaly);
    }
    
    // Calculate dwarf planet positions
    for (name, elements) in dwarf_planet_elements() {
        let mean_anomaly = elements.mean_anomaly_at_days(days_from_j2000);
        positions.insert(name, mean_anomaly);
    }
    
    positions
}

/// Calculate mean anomaly for a specific body at a given Unix timestamp
pub fn calculate_position_for_body(body_name: &str, unix_timestamp: i64) -> Option<f64> {
    // Convert Unix timestamp to days from J2000
    let days_from_unix_epoch = unix_timestamp as f64 / 86400.0;
    let days_from_j2000 = days_from_unix_epoch - DAYS_UNIX_EPOCH_TO_J2000;
    
    // Check planets first
    if let Some(elements) = planet_elements().get(body_name) {
        return Some(elements.mean_anomaly_at_days(days_from_j2000));
    }
    
    // Check moons
    if let Some(elements) = moon_elements().get(body_name) {
        return Some(elements.mean_anomaly_at_days(days_from_j2000));
    }
    
    // Check dwarf planets
    if let Some(elements) = dwarf_planet_elements().get(body_name) {
        return Some(elements.mean_anomaly_at_days(days_from_j2000));
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // January 1, 2026 00:00:00 UTC as Unix timestamp
    const JAN_1_2026_TIMESTAMP: i64 = 1_767_225_600;

    #[test]
    fn test_days_from_j2000_calculation() {
        // January 1, 2026 should be approximately 9496.5 days from J2000
        let days_from_unix = JAN_1_2026_TIMESTAMP as f64 / 86400.0;
        let days_from_j2000 = days_from_unix - DAYS_UNIX_EPOCH_TO_J2000;
        
        // Should be close to 9496.0 (the -0.5 accounts for J2000 being at 12:00, not 00:00)
        assert!((days_from_j2000 - 9496.0).abs() < 1.0, "Days from J2000 should be ~9496");
    }

    #[test]
    fn test_earth_position_jan_2026() {
        let positions = calculate_positions_at_timestamp(JAN_1_2026_TIMESTAMP);
        let earth_ma = positions.get("Earth").unwrap();
        
        // Earth's mean anomaly should be close to 356.86 degrees on Jan 1, 2026
        assert!((earth_ma - 356.86).abs() < 1.0, 
                "Earth MA should be ~356.86°, got {}", earth_ma);
    }

    #[test]
    fn test_jupiter_position_jan_2026() {
        let positions = calculate_positions_at_timestamp(JAN_1_2026_TIMESTAMP);
        let jupiter_ma = positions.get("Jupiter").unwrap();
        
        // Jupiter's mean anomaly should be close to 88.68 degrees on Jan 1, 2026
        assert!((jupiter_ma - 88.68).abs() < 1.0, 
                "Jupiter MA should be ~88.68°, got {}", jupiter_ma);
    }

    #[test]
    fn test_moon_position_jan_2026() {
        let positions = calculate_positions_at_timestamp(JAN_1_2026_TIMESTAMP);
        let moon_ma = positions.get("Moon").unwrap();
        
        // Moon's mean anomaly should be close to 337.70 degrees on Jan 1, 2026
        // Using larger tolerance since Moon calculation uses simplified base offset model
        assert!((moon_ma - 337.70).abs() < 10.0, 
                "Moon MA should be ~337.70°, got {}", moon_ma);
    }

    #[test]
    fn test_calculate_position_for_body() {
        let mercury_ma = calculate_position_for_body("Mercury", JAN_1_2026_TIMESTAMP);
        assert!(mercury_ma.is_some());
        
        let unknown_body = calculate_position_for_body("NonExistent", JAN_1_2026_TIMESTAMP);
        assert!(unknown_body.is_none());
    }

    #[test]
    fn test_normalize_angle() {
        // Test that angles are properly normalized to 0-360 range
        let positions = calculate_positions_at_timestamp(JAN_1_2026_TIMESTAMP);
        
        for (_name, angle) in positions.iter() {
            assert!(*angle >= 0.0 && *angle < 360.0, 
                    "Angle {} should be in range [0, 360)", angle);
        }
    }
}
