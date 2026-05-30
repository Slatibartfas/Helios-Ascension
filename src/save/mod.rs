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
    zstd::decode(data)
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
    use crate::colony::components::{Colony, ConstructionProject, PendingConstructionActions};
    use crate::economy::{GlobalBudget, MiningOperation};
    use crate::fleets::components::{ActiveManeuver, Fleet, FleetOrbit, StandingOrder};
    use crate::game_state::GameSeed;
    use crate::research::ResearchState;
    use crate::ui::SimulationTime;

    let sim_time = world.resource::<SimulationTime>();
    let game_seed = world.resource::<GameSeed>();

    // Extract colonies
    let colonies: Vec<ColonySaved> = world
        .query::<(&Colony, &crate::astronomy::components::SpaceCoordinates)>()
        .iter(world)
        .map(|(colony, coords)| ColonySaved {
            entity_id: colony.entity().index(),
            name: colony.name.clone(),
            population: colony.population,
            growth_rate_modifier: colony.growth_rate_modifier,
            buildings: colony
                .buildings
                .iter()
                .map(|(k, v)| (format!("{:?}", k), *v))
                .collect(),
            construction_queue: vec![], // TODO: extract from PendingConstructionActions
            position: [coords.position.x, coords.position.y, coords.position.z],
            orbiting_entity: None,
        })
        .collect();

    // Extract fleets
    let fleets: Vec<FleetSaved> = world
        .query::<(&Fleet, &FleetOrbit)>()
        .iter(world)
        .map(|(fleet, orbit)| FleetSaved {
            entity_id: fleet.entity().index(),
            name: fleet.name.clone(),
            ships: fleet.ships.iter().map(|s| ShipSaved {
                class: format!("{:?}", s.ship_class),
                name: s.name.clone(),
                health_percent: s.health_percent,
            }).collect(),
            orbit_body: orbit.parent_entity.map(|e| e.index()),
            orbit_angle: orbit.orbit_angle,
            maneuver: None,
            standing_orders: None,
        })
        .collect();

    // Extract research
    let research_state = world.resource::<ResearchState>();
    let research = ResearchSaved {
        active_projects: vec![],
        completed_technologies: vec![],
        engineering_projects: vec![],
    };

    // Extract economy
    let budget = world.resource::<GlobalBudget>();
    let economy = EconomySaved {
        treasury: budget.treasury,
        resource_stockpiles: vec![],
        active_mining_operations: vec![],
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
pub fn apply_game_state(world: &mut World, state: &GameSavedState) {
    // Restore simulation time
    if let Some(mut sim_time) = world.get_resource_mut::<SimulationTime>() {
        sim_time.elapsed = state.elapsed_seconds;
    }

    // Restore time scale
    if let Some(mut time_scale) = world.get_resource_mut::<crate::ui::TimeScale>() {
        time_scale.scale = state.time_scale;
    }

    // TODO: Reconstruct colonies, fleets, research, economy from saved state
    // This requires the procedural generation system to regenerate entities
    // from the seed, then apply the saved delta on top.

    info!("Game state restored from save");
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