//! Save/Load system for Helios Ascension
//!
//! Provides complete game state persistence using serde + zstd compression.
//! Includes quicksave, multiple save slots, autosave with rotation, and migration support.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

pub mod autosave;
pub mod migration;
pub mod slots;

/// Current save file version for migration handling
pub const SAVE_VERSION: u32 = 1;

/// File extension for compressed save files
pub const COMPRESSED_EXTENSION: &str = "ron.zst";

/// Default directory name for saves
pub const SAVE_DIR: &str = "saves";

/// Returns the save directory path, creating it if necessary.
pub fn save_directory() -> PathBuf {
    let base = get_config_dir();
    let dir = base.join(SAVE_DIR);
    if !dir.exists() {
        fs::create_dir_all(&dir).ok();
    }
    dir
}

/// Get platform-specific game config directory.
fn get_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        PathBuf::from(".")
    }
}

/// Raw save file data with metadata header.
// NOTE: This is version 1 of the save format. When the format changes,
// bump SAVE_VERSION and add migration handling in migration.rs.
#[derive(Serialize, Deserialize)]
struct SaveFileV1 {
    /// Version for format migration
    version: u32,
    /// When the save was created (Unix timestamp)
    timestamp: i64,
    /// Human-readable save description
    description: String,
    /// Game elapsed time in seconds
    elapsed_seconds: f64,
    /// Game seed for regeneration
    seed: u64,
    /// Serialized game state as JSON string
    state_json: String,
}

/// Top-level save file wrapper that handles compression + format versioning.
#[derive(Serialize, Deserialize)]
struct SaveFile {
    /// Save format version
    version: u32,
    /// Compressed and serialized SaveFileV1
    data: Vec<u8>,
}

impl SaveFile {
    fn new(state: &GameSavedState) -> std::io::Result<Self> {
        let mut json_bytes = serde_json::to_vec(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Compress with zstd (level 19 = best compression, 22 = extreme)
        let compressed = compress_zstd(&json_bytes)?;

        Ok(Self {
            version: SAVE_VERSION,
            data: compressed,
        })
    }

    fn into_saved_state(self) -> std::io::Result<GameSavedState> {
        let decompressed = decompress_zstd(&self.data)?;
        serde_json::from_slice(&decompressed)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// Compress data using zstd.
fn compress_zstd(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let compressed = zstd::encode_all(data, 19)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(compressed)
}

/// Decompress zstd data.
fn decompress_zstd(data: &[u8]) -> std::io::Result<Vec<u8>> {
    zstd::decode_all(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

/// Save the current game state to a file at the given path.
pub fn write_save(path: &PathBuf, state: &GameSavedState) -> std::io::Result<()> {
    let save = SaveFile::new(state)?;
    let bytes = serde_json::to_vec(&save)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut file = fs::File::create(path)?;
    file.write_all(&bytes)?;

    info!("Game saved to: {:?}", path);
    Ok(())
}

/// Load a game state from a file.
pub fn read_save(path: &PathBuf) -> std::io::Result<GameSavedState> {
    let bytes = fs::read(path)?;
    let save: SaveFile = serde_json::from_slice(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Apply migration if needed
    let save: SaveFileV1 = migration::migrate_save_file(save)?;

    info!(
        "Loading save: {} (elapsed: {:.1}s)",
        save.description,
        save.elapsed_seconds
    );

    serde_json::from_str(&save.state_json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Get current Unix timestamp.
pub fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Format a timestamp as a human-readable string.
pub fn format_timestamp(timestamp: i64) -> String {
    // Simple UTC formatting without external dependencies
    let days_since_epoch = timestamp / 86400;
    let secs_of_day = timestamp % 86400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;

    // Calculate year, month, day from days since epoch
    let mut days = days_since_epoch;
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let months_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &days_in_month in &months_days {
        if days >= days_in_month as i64 {
            days -= days_in_month as i64;
            month += 1;
        } else {
            break;
        }
    }

    let day = days + 1;

    format!(
        "{:02}.{:02}.{} {:02}:{:02}",
        day, month, year, hours, minutes
    )
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ─────────────────────────────────────────────────────────────────────────────
// GameSavedState - what we actually serialize
// ─────────────────────────────────────────────────────────────────────────────

/// Everything needed to reconstruct the game state.
/// This is the canonical "save game" format.
/// Note: Non-serializable things (meshes, textures, audio handles) are
/// regenerated from this state when loading.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameSavedState {
    /// Version of the save format
    pub version: u32,
    /// Unix timestamp when save was created
    pub timestamp: i64,
    /// Human-readable description
    pub description: String,
    /// Game seed for procedural regeneration
    pub seed: u64,
    /// Elapsed simulation time in seconds
    pub elapsed_seconds: f64,
    /// Time scale (paused, normal, accelerated)
    pub time_scale: f32,
    /// Colony data
    pub colonies: Vec<ColonySaved>,
    /// Fleet data
    pub fleets: Vec<FleetSaved>,
    /// Research progress
    pub research: ResearchSaved,
    /// Economy state
    pub economy: EconomySaved,
    /// Current star system
    pub current_system: usize,
}

/// Serializable colony state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColonySaved {
    pub entity_id: u32,
    pub name: String,
    pub population: f64,
    pub growth_rate_modifier: f64,
    pub buildings: Vec<(String, u32)>, // (building_type, count)
    pub construction_queue: Vec<ConstructionProjectSaved>,
    pub position: [f64; 3], // x, y, z in AU
    pub orbiting_entity: Option<u32>,
}

/// Serializable construction project.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConstructionProjectSaved {
    pub building_type: String,
    pub progress: f64,
    pub required_points: f64,
}

/// Serializable fleet state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FleetSaved {
    pub entity_id: u32,
    pub name: String,
    pub ships: Vec<ShipSaved>,
    pub orbit_body: Option<u32>,
    pub orbit_angle: f64,
    pub maneuver: Option<ManeuverSaved>,
    pub standing_orders: Option<StandingOrdersSaved>,
}

/// Serializable ship within a fleet.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShipSaved {
    pub class: String,
    pub name: String,
    pub health_percent: f32,
}

/// Serializable orbital maneuver.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManeuverSaved {
    pub transfer_type: String,
    pub target_entity: u32,
    pub start_time: f64,
    pub duration: f64,
    pub phase_angle: f64,
}

/// Serializable standing orders.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StandingOrdersSaved {
    pub return_when_idle: bool,
    pub rally_point: Option<u32>,
}

/// Serializable research state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResearchSaved {
    pub active_projects: Vec<ActiveProjectSaved>,
    pub completed_technologies: Vec<String>,
    pub engineering_projects: Vec<EngineeringProjectSaved>,
}

/// Serializable active research project.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActiveProjectSaved {
    pub tech_id: String,
    pub progress: f64,
    pub allocation_percent: f64,
}

/// Serializable engineering project.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EngineeringProjectSaved {
    pub component_id: String,
    pub progress: f64,
    pub allocation_percent: f64,
}

/// Serializable economy state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EconomySaved {
    pub treasury: f64,
    pub resource_stockpiles: Vec<(String, f64)>,
    pub active_mining_operations: Vec<MiningOperationSaved>,
}

/// Serializable mining operation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MiningOperationSaved {
    pub body_entity: u32,
    pub resource_type: String,
    pub extraction_rate: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// World extraction / application
// ─────────────────────────────────────────────────────────────────────────────

/// Extract all serializable state from the Bevy world.
pub fn extract_game_state(world: &World) -> GameSavedState {
    use crate::colony::components::{Colony, ConstructionProject};
    use crate::economy::{GlobalBudget, MiningOperation};
    use crate::fleets::components::{ActiveManeuver, Fleet, FleetOrbit};
    use crate::game_state::GameSeed;
    use crate::research::components::{EngineeringProject, ResearchProject};
    use crate::research::ResearchState;
    use crate::ui::SimulationTime;

    let sim_time = world.resource::<SimulationTime>();
    let game_seed = world.resource::<GameSeed>();

    // Build a set of known colony entity indices for filtering construction projects
    let colony_entity_ids: std::collections::HashSet<u32> = world
        .query::<&Colony>()
        .iter(world)
        .map(|c| c.entity().index())
        .collect();

    // Extract all construction projects linked to known colonies
    #[derive(Clone)]
    struct ProjWithColony {
        colony_entity_id: u32,
        saved: ConstructionProjectSaved,
    }
    let construction_projects: Vec<ProjWithColony> = world
        .query::<&ConstructionProject>()
        .iter(world)
        .filter(|proj| colony_entity_ids.contains(&proj.colony_entity.index()))
        .map(|proj| ProjWithColony {
            colony_entity_id: proj.colony_entity.index(),
            saved: ConstructionProjectSaved {
                building_type: format!("{:?}", proj.building_type),
                progress: proj.progress,
                required_points: proj.required,
            },
        })
        .collect();

    // Extract colonies with their construction queues
    let colonies: Vec<ColonySaved> = world
        .query::<(&Colony, &crate::astronomy::components::SpaceCoordinates)>()
        .iter(world)
        .map(|(colony, coords)| {
            let entity_id = colony.entity().index();
            let queue: Vec<ConstructionProjectSaved> = construction_projects
                .iter()
                .filter(|p| p.colony_entity_id == entity_id)
                .map(|p| p.saved.clone())
                .collect();
            ColonySaved {
                entity_id,
                name: colony.name.clone(),
                population: colony.population,
                growth_rate_modifier: colony.growth_rate_modifier,
                buildings: colony
                    .buildings
                    .iter()
                    .map(|(k, v)| (format!("{:?}", k), *v))
                    .collect(),
                construction_queue: queue,
                position: [coords.position.x, coords.position.y, coords.position.z],
                orbiting_entity: None,
            }
        })
        .collect();

    // Helper to get optional ActiveManeuver component as ManeuverSaved
    fn get_maneuver(world: &World, entity: Entity) -> Option<ManeuverSaved> {
        world.entity(entity).get::<ActiveManeuver>().map(|m| {
            ManeuverSaved {
                transfer_type: format!("{:?}", m.reference_frame),
                target_entity: m.orbit_center.index(),
                start_time: m.departure_time,
                duration: m.arrival_time - m.departure_time,
                phase_angle: m.transfer_orbit.mean_anomaly_epoch,
            }
        })
    }

    // Extract fleets with their active maneuvers
    let fleets: Vec<FleetSaved> = world
        .query::<(Entity, &Fleet, &FleetOrbit)>()
        .iter(world)
        .map(|(entity, fleet, orbit)| FleetSaved {
            entity_id: entity.index(),
            name: fleet.name.clone(),
            ships: fleet.ships.iter().map(|s| ShipSaved {
                class: format!("{:?}", s.class),
                name: s.name.clone(),
                health_percent: s.health_percent,
            }).collect(),
            orbit_body: Some(orbit.body.index()),
            orbit_angle: orbit.angle_rad,
            maneuver: get_maneuver(world, entity),
            standing_orders: None,
        })
        .collect();

    // Extract research state and active projects
    let research_state = world.resource::<ResearchState>();
    let active_projects: Vec<ActiveProjectSaved> = world
        .query::<&ResearchProject>()
        .iter(world)
        .map(|proj| ActiveProjectSaved {
            tech_id: proj.tech_id.clone(),
            progress: proj.progress,
            allocation_percent: proj.rp_allocation_percent,
        })
        .collect();
    let engineering_projects: Vec<EngineeringProjectSaved> = world
        .query::<&EngineeringProject>()
        .iter(world)
        .map(|proj| EngineeringProjectSaved {
            component_id: proj.component_id.clone(),
            progress: proj.progress,
            allocation_percent: 0.0,
        })
        .collect();

    let research = ResearchSaved {
        active_projects,
        completed_technologies: research_state.unlocked_technologies.iter().cloned().collect(),
        engineering_projects,
    };

    // Extract economy
    let budget = world.resource::<GlobalBudget>();
    let resource_stockpiles: Vec<(String, f64)> = budget
        .stockpiles
        .iter()
        .map(|(k, v)| (format!("{:?}", k), *v))
        .collect();
    let active_mining_operations: Vec<MiningOperationSaved> = world
        .query::<&MiningOperation>()
        .iter(world)
        .map(|op| {
            // Find the entity that has this MiningOperation component
            let body_entity = world
                .query_filtered::<Entity, With<MiningOperation>>()
                .iter(world)
                .find(|&e| {
                    world.entity(e)
                        .get::<MiningOperation>()
                        .map(|m| m.resource_type == op.resource_type)
                        .unwrap_or(false)
                })
                .unwrap_or(Entity::PLACEHOLDER);
            MiningOperationSaved {
                body_entity: body_entity.index(),
                resource_type: format!("{:?}", op.resource_type),
                extraction_rate: op.base_rate_mt_per_year,
            }
        })
        .collect();

    let economy = EconomySaved {
        treasury: budget.treasury,
        resource_stockpiles,
        active_mining_operations,
    };

    GameSavedState {
        version: SAVE_VERSION,
        timestamp: current_timestamp(),
        description: "Auto-save".to_string(),
        seed: game_seed.value,
        elapsed_seconds: sim_time.elapsed_seconds(),
        time_scale: world.resource::<crate::ui::TimeScale>().scale,
        colonies,
        fleets,
        research,
        economy,
        current_system: 0,
    }
}

/// Apply saved state to a Bevy world (reconstruct entities, resources).
///
/// The solar system is regenerated from the seed, then colonies, fleets,
/// research and economy are reconstructed on top. Entity IDs in the save
/// do not match the regenerated entities — matching is done by name and
/// orbital position instead.
pub fn apply_game_state(world: &mut World, state: &GameSavedState) {
    use crate::astronomy::components::SpaceCoordinates;
    use crate::colony::components::{Colony, PendingConstructionActions};
    use crate::economy::mining::MiningOperation;
    use crate::fleets::components::{ActiveManeuver, Fleet, FleetOrbit};
    use crate::fleets::{FleetRole, ShipInfo, PropulsionType};
    use crate::game_state::GameSeed;
    use crate::plugins::solar_system::CelestialBody;
    use crate::research::ResearchState;
    use crate::ui::SimulationTime;

    // Restore simulation time
    if let Some(mut sim_time) = world.get_resource_mut::<SimulationTime>() {
        sim_time.elapsed = state.elapsed_seconds;
    }

    // Restore time scale
    if let Some(mut time_scale) = world.get_resource_mut::<crate::ui::TimeScale>() {
        time_scale.scale = state.time_scale;
    }

    // Despawn all existing colonies, fleets, and research project entities so
    // they can be rebuilt cleanly.
    let mut despawn_colonies = Vec::new();
    let mut despawn_fleets = Vec::new();
    let mut despawn_research_projects = Vec::new();

    for (entity, _) in world.query::<&Colony>().iter() {
        despawn_colonies.push(entity);
    }
    for (entity, _) in world.query::<&Fleet>().iter() {
        despawn_fleets.push(entity);
    }
    for (entity, _) in world.query::<&crate::research::ResearchProject>().iter() {
        despawn_research_projects.push(entity);
    }

    let mut commands = world.commands();
    for entity in despawn_colonies {
        commands.entity(entity).despawn();
    }
    for entity in despawn_fleets {
        commands.entity(entity).despawn();
    }
    for entity in despawn_research_projects {
        commands.entity(entity).despawn();
    }
    commands.flush();

    // Update the game seed so the solar system regenerates identically
    if let Some(mut seed) = world.get_resource_mut::<GameSeed>() {
        seed.value = state.seed;
    }

    // Clear any pending actions left from the partial teardown above.
    world.resource_mut::<PendingConstructionActions>().clear();
    world.resource_mut::<crate::fleets::PendingFleetActions>().clear();

    // ── Build body lookup by name ───────────────────────────────────────────
    let mut body_by_name: std::collections::HashMap<String, Entity> =
        std::collections::HashMap::new();
    let body_entities: Vec<Entity> = world
        .query::<&CelestialBody>()
        .iter(world)
        .map(|(e, b)| {
            body_by_name.insert(b.name.clone(), e);
            e
        })
        .collect();

    // ── Reconstruct colonies ───────────────────────────────────────────────
    let construction_actions = world.resource_mut::<PendingConstructionActions>();

    for colony_data in &state.colonies {
        // Try to find the body by name first, then by position.
        let body_entity = body_by_name
            .get(&colony_data.name)
            .copied()
            .or_else(|| {
                body_entities.iter().find_map(|&e| {
                    let coords = world.get::<SpaceCoordinates>(e)?;
                    let pos = coords.position;
                    let dp = [
                        (pos.x - colony_data.position[0]).abs(),
                        (pos.y - colony_data.position[1]).abs(),
                        (pos.z - colony_data.position[2]).abs(),
                    ];
                    if dp[0] < 0.1 && dp[1] < 0.1 && dp[2] < 0.1 {
                        Some(e)
                    } else {
                        None
                    }
                })
            });

        let Some(body_entity) = body_entity else {
            warn!(
                "apply_game_state: colony '{}' body not found, skipping",
                colony_data.name
            );
            continue;
        };

        // Build colony buildings map — parse building type strings
        let buildings: std::collections::HashMap<
            crate::colony::BuildingType,
            u32,
        > = colony_data
            .buildings
            .iter()
            .filter_map(|(bt_str, count)| bt_str.parse().ok().map(|bt| (bt, *count)))
            .collect();

        let colony_entity = commands
            .spawn((
                Colony {
                    name: colony_data.name.clone(),
                    population: colony_data.population,
                    growth_rate_modifier: colony_data.growth_rate_modifier,
                    buildings,
                },
                SpaceCoordinates::new(bevy::math::DVec3::new(
                    colony_data.position[0],
                    colony_data.position[1],
                    colony_data.position[2],
                )),
            ))
            .id();

        // Rebuild construction queue
        for project in &colony_data.construction_queue {
            if let Ok(bt) = project.building_type.parse() {
                construction_actions.start_construction.push((colony_entity, bt));
            }
        }
    }
    commands.flush();

    // ── Reconstruct fleets ──────────────────────────────────────────────────

    for fleet_data in &state.fleets {
        // Find orbit body by saved entity index; fall back to first body (star).
        let orbit_body = fleet_data
            .orbit_body
            .and_then(|old_id| {
                body_entities
                    .iter()
                    .find(|&&e| e.index() == old_id as usize)
                    .copied()
            })
            .unwrap_or_else(|| {
                body_entities.first().copied().unwrap_or(Entity::PLACEHOLDER)
            });

        let ships: Vec<ShipInfo> = fleet_data
            .ships
            .iter()
            .filter_map(|s| {
                s.class.parse().ok().map(|class| {
                    ShipInfo::new(s.name.clone(), class, PropulsionType::Fusion)
                })
            })
            .collect();

        let fleet_entity = commands
            .spawn((
                Fleet {
                    name: fleet_data.name.clone(),
                    role: FleetRole::default(),
                    ships,
                },
                FleetOrbit {
                    body: orbit_body,
                    radius_au: 0.01,
                    angle_rad: fleet_data.orbit_angle,
                    direction: 1.0,
                },
            ))
            .id();

        // Reconstruct active maneuver
        if let Some(maneuver) = &fleet_data.maneuver {
            let target = body_entities
                .iter()
                .find(|&&e| e.index() == maneuver.target_entity as usize)
                .copied()
                .unwrap_or(Entity::PLACEHOLDER);

            let transfer_orbit = crate::astronomy::KeplerOrbit::new(
                0.0,
                (maneuver.phase_angle / 100.0).max(0.01),
                0.0,
                0.0,
                0.0,
                maneuver.phase_angle,
                std::f64::consts::TAU / maneuver.duration.max(1.0),
            );

            if let Ok(cmd) = commands.get_entity(fleet_entity) {
                cmd.insert(ActiveManeuver {
                    transfer_orbit,
                    reference_frame: crate::fleets::TransferReferenceFrame::Body(orbit_body),
                    orbit_center: orbit_body,
                    origin_body: orbit_body,
                    departure_time: maneuver.start_time,
                    arrival_time: maneuver.start_time + maneuver.duration,
                    preserve_orbit_geometry: false,
                    destination_body: target,
                    arrival_orbit_radius_au: 0.01,
                    arrival_delta_v_ms: 0.0,
                    fuel_used_t: 0.0,
                    option_label: "Restored",
                    departure_angle: 0.0,
                    start_position_au: None,
                    end_position_au: None,
                    departure_velocity_ms: None,
                    arrival_velocity_ms: None,
                    start_visual_pos: None,
                    leg2_orbit: None,
                    leg2_start_s: 0.0,
                    flyby_body: None,
                });
            }
        }
    }
    commands.flush();

    // ── Reconstruct research state ────────────────────────────────────────
    if let Some(mut research_state) = world.get_resource_mut::<ResearchState>() {
        for tech_id in &state.research.completed_technologies {
            research_state.unlocked_technologies.insert(tech_id.clone());
        }
        // Note: active projects require spawning ResearchProject + ResearchTeam
        // entities which is more involved and left for a follow-up issue.
    }

    // ── Reconstruct economy state ─────────────────────────────────────────
    if let Some(mut budget) = world.get_resource_mut::<crate::economy::GlobalBudget>() {
        budget.treasury = state.economy.treasury;
        for (resource_str, amount) in &state.economy.resource_stockpiles {
            if let Ok(resource) = resource_str.parse() {
                budget.stockpiles.insert(resource, *amount);
            }
        }
        // Rebuild mining operations: attach MiningOperation to body entities
        for mining in &state.economy.active_mining_operations {
            if let Some(body) = body_entities
                .iter()
                .find(|&&e| e.index() == mining.body_entity as usize)
                .copied()
            {
                if let Ok(cmd) = commands.get_entity(body) {
                    cmd.insert(MiningOperation {
                        body,
                        resource_type: mining
                            .resource_type
                            .parse()
                            .unwrap_or(crate::economy::ResourceType::Silicates),
                        base_rate_mt_per_year: mining.extraction_rate,
                        active: true,
                    });
                }
            }
        }
    }
    commands.flush();

    info!(
        "Game state restored from save: {} colonies, {} fleets, seed={}",
        state.colonies.len(),
        state.fleets.len(),
        state.seed
    );
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ─────────────────────────────────────────────────────────────────────────────
// SaveLoadActions - Queue for save/load operations triggered by UI
// ─────────────────────────────────────────────────────────────────────────────

/// Actions requested by UI that will be processed by a system.
#[derive(Resource, Default, Debug)]
pub struct SaveLoadActions {
    /// If Some, trigger a save to this slot
    pub save_to_slot: Option<(usize, String)>,
    /// If Some, trigger a load from this slot
    pub load_from_slot: Option<usize>,
}

impl SaveLoadActions {
    /// Request a save to a specific slot with a name.
    pub fn request_save(&mut self, slot: usize, name: String) {
        self.save_to_slot = Some((slot, name));
    }

    /// Request a load from a specific slot.
    pub fn request_load(&mut self, slot: usize) {
        self.load_from_slot = Some(slot);
    }

    /// Clear all pending actions.
    pub fn clear(&mut self) {
        self.save_to_slot = None;
        self.load_from_slot = None;
    }
}

/// System to process save/load actions.
/// Called in Update schedule to handle queued operations.
pub fn process_save_load_actions(
    world: &mut World,
    mut actions: ResMut<SaveLoadActions>,
) {
    // Handle save request
    if let Some((slot, name)) = actions.save_to_slot.take() {
        let state = extract_game_state(world);
        match slots::save_to_slot(slot, &state, &name) {
            Ok(metadata) => info!("Game saved to slot {}: {}", slot, metadata.name),
            Err(e) => error!("Failed to save game: {:?}", e),
        }
    }

    // Handle load request
    if let Some(slot) = actions.load_from_slot.take() {
        match slots::load_from_slot(slot) {
            Ok(state) => {
                apply_game_state(world, &state);
                info!("Game loaded from slot {}", slot);
            }
            Err(e) => error!("Failed to load game: {:?}", e),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SavePlugin - Bevy plugin for save/load integration
// ─────────────────────────────────────────────────────────────────────────────

/// Bevy plugin that integrates the save/load system into the game.
pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveLoadActions>()
            .add_systems(Update, (autosave::autosave_system, process_save_load_actions));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_file_roundtrip() {
        let state = GameSavedState {
            version: 1,
            timestamp: 1234567890,
            description: "Test save".to_string(),
            seed: 42,
            elapsed_seconds: 3600.0,
            time_scale: 1.0,
            colonies: vec![],
            fleets: vec![],
            research: ResearchSaved {
                active_projects: vec![],
                completed_technologies: vec![],
                engineering_projects: vec![],
            },
            economy: EconomySaved {
                treasury: 1000.0,
                resource_stockpiles: vec![],
                active_mining_operations: vec![],
            },
            current_system: 0,
        };

        let save = SaveFile::new(&state).unwrap();
        let bytes = serde_json::to_vec(&save).unwrap();
        let loaded: SaveFile = serde_json::from_slice(&bytes).unwrap();
        let restored: GameSavedState = loaded.into_saved_state().unwrap();

        assert_eq!(restored.seed, 42);
        assert_eq!(restored.elapsed_seconds, 3600.0);
    }

    #[test]
    fn test_format_timestamp() {
        // Jan 1, 2026 00:00:00 UTC
        let ts = 1_767_225_600i64;
        let formatted = format_timestamp(ts);
        assert!(formatted.contains("2026"));
    }
}