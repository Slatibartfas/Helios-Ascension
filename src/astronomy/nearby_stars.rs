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

#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
pub struct NearbyStarsData {
    pub systems: Vec<StarSystemData>,
}

impl NearbyStarsData {
    pub fn get_by_name(&self, name: &str) -> Option<&StarSystemData> {
        self.systems.iter().find(|s| s.system_name == name)
    }

    pub fn get_by_id(&self, id: usize) -> Option<&StarSystemData> {
        // ID 0 is Sol (not in this data).
        // IDs 1+ correspond to NEARBY_STARS_POSITIONS ordering, NOT the JSON
        // array index. Look up the system name from the static positions
        // table and then find the matching entry in the loaded JSON data.
        if id == 0 {
            return None;
        }
        let name = NEARBY_STARS_POSITIONS.get(id - 1)?.name;
        self.systems.iter().find(|s| s.system_name == name)
    }
}

#[derive(Debug, Clone, Deserialize, Reflect)]
pub struct StarSystemData {
    pub system_name: String,
    pub distance_ly: f32,
    pub stars: Vec<StarData>,
    /// Binary/multiple star orbital parameters
    #[serde(default)]
    pub binary_orbits: Vec<BinaryOrbitData>,
}

#[derive(Debug, Clone, Deserialize, Reflect)]
pub struct StarData {
    pub name: String,
    pub spectral_type: String,
    pub mass_sol: f32,
    pub radius_sol: f32,
    pub temp_k: f32,
    pub luminosity_sol: f32,
    /// Stellar metallicity [Fe/H] relative to the Sun
    /// Sun = 0.0, positive = metal-rich, negative = metal-poor
    /// Optional: will use random value if not provided
    #[serde(default)]
    pub metallicity: Option<f32>,
    #[serde(default)]
    pub planets: Vec<PlanetData>,
}

#[derive(Debug, Clone, Deserialize, Reflect)]
pub struct PlanetData {
    pub name: String,
    pub mass_earth: f32,
    pub radius_earth: Option<f32>,
    pub period_days: f32,
    pub semi_major_axis_au: f32,
    pub eccentricity: f32,
    #[serde(rename = "type")]
    pub planet_type: String,
    /// Optional index of the star this planet orbits (0 = first star, 1 = second, etc.).
    /// When omitted, the planet is assumed to orbit the star record that contains it.
    #[serde(default)]
    pub orbits_star: Option<usize>,
}

/// Binary star orbital relationship
#[derive(Debug, Clone, Deserialize, Reflect)]
pub struct BinaryOrbitData {
    /// Name/label for this orbital pair
    pub label: String,
    /// Optional index of the primary body in the stars array.
    /// Omit when the primary is another orbit label.
    #[serde(default)]
    pub primary_idx: Option<usize>,
    /// Optional orbit label used as the primary body.
    #[serde(default)]
    pub primary_orbit_label: Option<String>,
    /// Optional index of the secondary body in the stars array.
    /// Omit when the secondary is another orbit label.
    #[serde(default)]
    pub secondary_idx: Option<usize>,
    /// Optional orbit label used as the secondary body.
    #[serde(default)]
    pub secondary_orbit_label: Option<String>,
    /// Semi-major axis of the binary orbit in AU
    pub semi_major_axis_au: f64,
    /// Orbital period in years
    pub period_years: f64,
    /// Eccentricity of the binary orbit
    pub eccentricity: f64,
    /// Inclination in degrees
    #[serde(default)]
    pub inclination_deg: f64,
    /// Longitude of ascending node in degrees
    #[serde(default)]
    pub longitude_ascending_node_deg: f64,
    /// Argument of periastron in degrees
    #[serde(default)]
    pub arg_periastron_deg: f64,
}

pub fn load_nearby_stars_data(mut stars_data: ResMut<NearbyStarsData>) {
    let path = Path::new("assets/data/nearest_stars_raw.json");
    match fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<Vec<StarSystemData>>(&content) {
            Ok(data) => {
                info!("Loaded data for {} nearby star systems.", data.len());
                stars_data.systems = data;
            }
            Err(e) => error!("Failed to parse nearby stars data: {}", e),
        },
        Err(e) => warn!("Could not read nearby stars data file: {}", e),
    }
}

#[derive(Debug, Clone)]
pub struct StarPositionData {
    // `&'static str` is not Reflect-friendly; this table is purely a compile-time
    // lookup for star IDs → names, never persisted across save/load, so the fields
    // stay excluded from the reflect pipeline by not having a `Reflect` derive.
    pub name: &'static str,
    pub pos_ly: [f64; 3],            // x, y, z in Light Years
    pub spectral_type: &'static str, // For color
}

// 50 Closest Star Systems to Sol (excluding Sol)
// Coordinates in Light Years (Equatorial J2000 Cartesian)
pub const NEARBY_STARS_POSITIONS: &[StarPositionData] = &[
    StarPositionData {
        name: "Alpha Centauri",
        pos_ly: [-1.5477, -1.1846, -3.7728],
        spectral_type: "G2V",
    },
    StarPositionData {
        name: "Barnard's Star",
        pos_ly: [-0.0568, -5.9426, 0.4879],
        spectral_type: "M4.0Ve",
    },
    StarPositionData {
        name: "Luhman 16",
        pos_ly: [-3.7012, 1.1792, -5.2152],
        spectral_type: "L8",
    },
    StarPositionData {
        name: "WISE 0855-0714",
        pos_ly: [-5.1011, 5.3203, -0.9371],
        spectral_type: "Y4",
    },
    StarPositionData {
        name: "Wolf 359",
        pos_ly: [-7.4995, 2.1332, 0.9594],
        spectral_type: "M6.0V",
    },
    StarPositionData {
        name: "Lalande 21185",
        pos_ly: [-6.5166, 1.6448, 4.8777],
        spectral_type: "M2.0V",
    },
    StarPositionData {
        name: "Sirius",
        pos_ly: [-1.6326, 8.18, -2.5051],
        spectral_type: "A1V",
    },
    StarPositionData {
        name: "Luyten 726-8",
        pos_ly: [7.5367, 3.4753, -2.6887],
        spectral_type: "M5.5Ve",
    },
    StarPositionData {
        name: "Ross 154",
        pos_ly: [1.915, -8.6694, -3.9225],
        spectral_type: "M3.5Ve",
    },
    StarPositionData {
        name: "Ross 248",
        pos_ly: [7.3684, -0.5828, 7.1815],
        spectral_type: "M5.5Ve",
    },
    StarPositionData {
        name: "Epsilon Eridani",
        pos_ly: [6.1847, 8.2771, -1.7213],
        spectral_type: "K2V",
    },
    StarPositionData {
        name: "Lacaille 9352",
        pos_ly: [8.4508, -2.0341, -6.2812],
        spectral_type: "M0.5V",
    },
    StarPositionData {
        name: "Ross 128",
        pos_ly: [-10.9906, 0.5885, 0.1545],
        spectral_type: "M4.0Vn",
    },
    StarPositionData {
        name: "EZ Aquarii",
        pos_ly: [10.0458, -3.7282, -2.9312],
        spectral_type: "M5.0Ve",
    },
    StarPositionData {
        name: "61 Cygni",
        pos_ly: [6.4753, -6.0967, 7.1379],
        spectral_type: "K5.0V",
    },
    StarPositionData {
        name: "Procyon",
        pos_ly: [-4.7928, 10.3605, 1.0439],
        spectral_type: "F5IV-V",
    },
    StarPositionData {
        name: "Struve 2398",
        pos_ly: [1.0781, -5.7086, 9.914],
        spectral_type: "M3.0V",
    },
    StarPositionData {
        name: "Groombridge 34",
        pos_ly: [8.328, 0.6694, 8.0747],
        spectral_type: "M1.5V",
    },
    StarPositionData {
        name: "DX Cancri",
        pos_ly: [-6.3414, 8.2773, 5.2619],
        spectral_type: "M6.5Ve",
    },
    StarPositionData {
        name: "Epsilon Indi",
        pos_ly: [5.6765, -3.1673, -9.9283],
        spectral_type: "K5Ve",
    },
    StarPositionData {
        name: "Tau Ceti",
        pos_ly: [10.2932, 5.0241, -3.2708],
        spectral_type: "G8.5V",
    },
    StarPositionData {
        name: "GJ 1061",
        pos_ly: [5.0232, 6.9135, -8.4015],
        spectral_type: "M5.5V",
    },
    StarPositionData {
        name: "YZ Ceti",
        pos_ly: [11.0172, 3.6068, -3.544],
        spectral_type: "M4.5V",
    },
    StarPositionData {
        name: "Luyten's Star",
        pos_ly: [-4.5772, 11.4136, 1.1247],
        spectral_type: "M3.5V",
    },
    StarPositionData {
        name: "Teegarden's Star",
        pos_ly: [8.7097, 8.1943, 3.629],
        spectral_type: "M6.5V",
    },
    StarPositionData {
        name: "Kapteyn's Star",
        pos_ly: [1.8982, 8.869, -9.0756],
        spectral_type: "M1.5V",
    },
    StarPositionData {
        name: "Lacaille 8760",
        pos_ly: [7.6441, -6.5718, -8.1246],
        spectral_type: "M0.0V",
    },
    StarPositionData {
        name: "SCR 1845-6357",
        pos_ly: [1.1209, -5.6237, -11.738],
        spectral_type: "M8.5V",
    },
    StarPositionData {
        name: "Kruger 60",
        pos_ly: [6.4306, -2.7299, 11.0491],
        spectral_type: "M3.0V",
    },
    StarPositionData {
        name: "DENIS J1048-3956",
        pos_ly: [-9.6244, 3.1158, -8.469],
        spectral_type: "M8.5V",
    },
    StarPositionData {
        name: "Ross 614",
        pos_ly: [-1.7069, 13.2373, -0.656],
        spectral_type: "M4.5V",
    },
    StarPositionData {
        name: "UGPS J0722-0540",
        pos_ly: [-4.7051, 12.5085, -1.328],
        spectral_type: "T9",
    },
    StarPositionData {
        name: "Wolf 1061",
        pos_ly: [-5.2293, -12.6717, -3.0799],
        spectral_type: "M3.0V",
    },
    StarPositionData {
        name: "Van Maanen's Star",
        pos_ly: [13.6885, 2.9824, 1.3215],
        spectral_type: "DZ7",
    },
    StarPositionData {
        name: "Gliese 1",
        pos_ly: [11.2638, 0.2658, -8.601],
        spectral_type: "M1.5V",
    },
    StarPositionData {
        name: "TZ Arietis",
        pos_ly: [12.2919, 7.1125, 3.2923],
        spectral_type: "M4.5V",
    },
    StarPositionData {
        name: "Wolf 424",
        pos_ly: [-14.2627, -2.0862, 2.2884],
        spectral_type: "M5.5V",
    },
    StarPositionData {
        name: "Gliese 687",
        pos_ly: [-0.5623, -5.4485, 13.7916],
        spectral_type: "M3.0V",
    },
    StarPositionData {
        name: "Gliese 674",
        pos_ly: [-1.383, -10.0523, -10.8415],
        spectral_type: "M3.0V",
    },
    StarPositionData {
        name: "LHS 292",
        pos_ly: [-13.8709, 4.4929, -2.9233],
        spectral_type: "M6.5V",
    },
    StarPositionData {
        name: "Gliese 440",
        pos_ly: [-6.4165, 0.4005, -13.688],
        spectral_type: "DQ6",
    },
    StarPositionData {
        name: "GJ 1245",
        pos_ly: [5.1766, -9.5437, 10.6378],
        spectral_type: "M5.5V",
    },
    StarPositionData {
        name: "WISE 1741+2553",
        pos_ly: [-1.1098, -13.6475, 6.6454],
        spectral_type: "T9",
    },
    StarPositionData {
        name: "Gliese 876",
        pos_ly: [14.147, -4.239, -3.7544],
        spectral_type: "M3.5V",
    },
    StarPositionData {
        name: "WISE 1639-6847",
        pos_ly: [-1.9044, -5.2097, -14.2977],
        spectral_type: "Y0.5",
    },
    StarPositionData {
        name: "LHS 288",
        pos_ly: [-7.1797, 2.4598, -13.8107],
        spectral_type: "M5.5V",
    },
    StarPositionData {
        name: "GJ 1002",
        pos_ly: [15.6626, 0.4601, -2.0739],
        spectral_type: "M5.5V",
    },
    StarPositionData {
        name: "DENIS 0255-4700",
        pos_ly: [7.8177, 7.4878, -11.6144],
        spectral_type: "L7.5V",
    },
    StarPositionData {
        name: "Groombridge 1618",
        pos_ly: [-9.1881, 4.7135, 12.0713],
        spectral_type: "K7.0V",
    },
    StarPositionData {
        name: "Gliese 412",
        pos_ly: [-11.2719, 2.7334, 11.0169],
        spectral_type: "M1.0V",
    },
    // ── Systems added to match nearest_stars_raw.json ──────────────
    StarPositionData {
        name: "AD Leonis",
        pos_ly: [-8.2064, 5.0891, 12.4482],
        spectral_type: "M3.5Ve",
    },
    StarPositionData {
        name: "GJ 3323",
        pos_ly: [5.5742, 13.2783, -8.1621],
        spectral_type: "M4.0V",
    },
    StarPositionData {
        name: "Gliese 526",
        pos_ly: [-12.2104, 4.0766, 11.8312],
        spectral_type: "M1.5V",
    },
    StarPositionData {
        name: "Stein 2051",
        pos_ly: [3.1682, -5.1839, 17.0252],
        spectral_type: "M4.0V",
    },
    StarPositionData {
        name: "Gliese 251",
        pos_ly: [-6.4685, 10.8082, 11.6925],
        spectral_type: "M3.0V",
    },
    StarPositionData {
        name: "Gliese 908",
        pos_ly: [15.8429, -1.4821, 8.9476],
        spectral_type: "M1.0V",
    },
    StarPositionData {
        name: "Gliese 752",
        pos_ly: [5.0133, -15.0101, 10.2773],
        spectral_type: "M2.5V",
    },
    StarPositionData {
        name: "82 G. Eridani",
        pos_ly: [11.0048, 12.8764, -8.9236],
        spectral_type: "G8V",
    },
    StarPositionData {
        name: "Delta Pavonis",
        pos_ly: [4.5825, -5.2689, -18.5862],
        spectral_type: "G8IV",
    },
    StarPositionData {
        name: "Gliese 581",
        pos_ly: [-5.0516, -15.6627, -11.5688],
        spectral_type: "M3.0V",
    },
];

impl NearbyStarsData {
    pub fn get_position_by_name(name: &str) -> Option<[f64; 3]> {
        NEARBY_STARS_POSITIONS
            .iter()
            .find(|star| star.name == name)
            .map(|star| star.pos_ly)
    }

    /// Returns the starmap-compatible system ID for the given system name.
    /// The starmap assigns IDs as `index + 1` (0 = Sol) based on the order in
    /// `NEARBY_STARS_POSITIONS`. The populator **must** use this function
    /// instead of sequential counters so that entity `SystemId` values match
    /// the starmap icon IDs, ensuring the floating origin is set correctly
    /// when transitioning between systems.
    pub fn get_system_id_by_name(name: &str) -> Option<usize> {
        NEARBY_STARS_POSITIONS
            .iter()
            .position(|star| star.name == name)
            .map(|idx| idx + 1) // 0 = Sol
    }
}

#[cfg(test)]
mod tests {
    use super::BinaryOrbitData;

    #[test]
    fn test_hierarchical_binary_orbit_deserializes() {
        let orbit: BinaryOrbitData = serde_json::from_str(
            r#"{
                "label": "Alpha Centauri Outer",
                "primary_orbit_label": "Alpha Centauri AB",
                "secondary_idx": 2,
                "semi_major_axis_au": 8700.0,
                "period_years": 547000.0,
                "eccentricity": 0.5,
                "inclination_deg": 107.6,
                "arg_periastron_deg": 0.0
            }"#,
        )
        .expect("hierarchical orbit should deserialize");

        assert_eq!(
            orbit.primary_orbit_label.as_deref(),
            Some("Alpha Centauri AB")
        );
        assert_eq!(orbit.secondary_idx, Some(2));
        assert_eq!(orbit.primary_idx, None);
    }
}
