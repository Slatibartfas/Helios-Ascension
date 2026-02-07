use bevy::prelude::*;
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub struct NearbyStarsPlugin;

impl Plugin for NearbyStarsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyStarsData>()
           .add_systems(Startup, load_nearby_stars_data);
    }
}

#[derive(Resource, Default)]
pub struct NearbyStarsData {
    pub systems: Vec<StarSystemData>,
}

impl NearbyStarsData {
    pub fn get_by_name(&self, name: &str) -> Option<&StarSystemData> {
        self.systems.iter().find(|s| s.system_name == name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StarSystemData {
    pub system_name: String,
    pub distance_ly: f32,
    pub stars: Vec<StarData>,
    /// Binary/multiple star orbital parameters
    #[serde(default)]
    pub binary_orbits: Vec<BinaryOrbitData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StarData {
    pub name: String,
    pub spectral_type: String,
    pub mass_sol: f32,
    pub radius_sol: f32,
    pub temp_k: f32,
    pub luminosity_sol: f32,
    #[serde(default)]
    pub planets: Vec<PlanetData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanetData {
    pub name: String,
    pub mass_earth: f32,
    pub radius_earth: Option<f32>,
    pub period_days: f32,
    pub semi_major_axis_au: f32,
    pub eccentricity: f32,
    #[serde(rename = "type")]
    pub planet_type: String,
    /// Index of the star this planet orbits (0 = first star, 1 = second, etc.)
    #[serde(default)]
    pub orbits_star: usize,
}

/// Binary star orbital relationship
#[derive(Debug, Clone, Deserialize)]
pub struct BinaryOrbitData {
    /// Name/label for this orbital pair
    pub label: String,
    /// Index of the primary body in the stars array
    pub primary_idx: usize,
    /// Index of the secondary body in the stars array
    pub secondary_idx: usize,
    /// Semi-major axis of the binary orbit in AU
    pub semi_major_axis_au: f64,
    /// Orbital period in years
    pub period_years: f64,
    /// Eccentricity of the binary orbit
    pub eccentricity: f64,
    /// Inclination in degrees
    #[serde(default)]
    pub inclination_deg: f64,
    /// Argument of periastron in degrees
    #[serde(default)]
    pub arg_periastron_deg: f64,
}

fn load_nearby_stars_data(mut stars_data: ResMut<NearbyStarsData>) {
    let path = Path::new("assets/data/nearest_stars_raw.json");
    match fs::read_to_string(path) {
        Ok(content) => {
            match serde_json::from_str::<Vec<StarSystemData>>(&content) {
                Ok(data) => {
                    info!("Loaded data for {} nearby star systems.", data.len());
                    stars_data.systems = data;
                },
                Err(e) => error!("Failed to parse nearby stars data: {}", e),
            }
        },
        Err(e) => warn!("Could not read nearby stars data file: {}", e),
    }
}
