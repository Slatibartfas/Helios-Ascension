//! Save file format migration.
//!
//! Handles upgrading save files from older versions to newer formats.

use super::SaveFile;

/// Migrate a save file to the current version.
/// Returns the migrated SaveFileV1 with all version differences resolved.
pub fn migrate_save_file(save: SaveFile) -> std::io::Result<super::SaveFileV1> {
    match save.version {
        1 => {
            // Version 1 is the current format
            // Decompress and deserialize the inner data
            let decompressed = super::decompress_zstd(&save.data)?;
            let inner: super::SaveFileV1 = serde_json::from_slice(&decompressed)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(inner)
        }
        // Add future migrations here:
        // 2 => migrate_from_v2_to_v1(save),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unknown save version: {}", save.version),
        )),
    }
}

/// Migration descriptor for tracking version differences.
#[derive(Debug)]
pub struct Migration {
    /// Source version
    pub from_version: u32,
    /// Target version
    pub to_version: u32,
    /// Description of what changed
    pub description: String,
}

/// Returns all available migrations.
pub fn available_migrations() -> Vec<Migration> {
    vec![
        // Future migrations go here
        // Migration { from_version: 1, to_version: 2, description: "Added new field X" },
    ]
}