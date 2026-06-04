//! Era definitions and loader.
//!
//! Eras are the **coarse strategic buckets** in the campaign progression
//! (Foundations → Space Operations → Propulsion → Habitation & Colonization
//! → Defense & Industry). The canonical 5-era framing ships in
//! `assets/data/eras.ron` and is loaded here as a typed `ErasData`
//! `Resource`.
//!
//! The CTO schema (DELA-6) added:
//! * [`crate::research::TechEra`] enum in `types.rs` with `display_name()`,
//!   `icon()`, and `all()` helpers.
//! * `Technology::era: Option<TechEra>` on every `Technology` row.
//! * This module: the `Era` struct, the `ErasData` resource, and
//!   `load_eras` for `DataLoaderPlugin` to dispatch to.
//!
//! Assigning each of the existing 303 techs to an era is LGD scope (see
//! `PROPULSION_ERA_TECH_TREE.md` §8 / DELA-5 follow-ups); the schema is
//! the CTO deliverable.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::types::TechEra;

/// One era definition loaded from `assets/data/eras.ron`.
///
/// The `id` is the variant of [`TechEra`] the row describes. The human
/// display name and icon are derived from the enum, not stored in the
/// RON file, so changing the enum centralizes the player-facing strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Era {
    /// The era this row describes.
    pub id: TechEra,
    /// LGD-authored description of what this era gates for the player.
    pub description: String,
    /// Game year the era starts at (inclusive).
    pub start_year: u32,
    /// Game year the era ends at (exclusive). `None` for the final era.
    #[serde(default)]
    pub end_year: Option<u32>,
}

/// Resource holding every era definition loaded from `assets/data/eras.ron`.
///
/// Indexed by [`TechEra`] for O(1) lookup from the UI, the tech tree, and
/// the future research-tick system. `Default` produces an empty map so
/// downstream systems can read `Res<ErasData>` without worrying about
/// whether the loader ran yet.
#[derive(Resource, Debug, Clone, Default)]
pub struct ErasData {
    /// All eras indexed by their `TechEra` discriminant.
    pub eras: HashMap<TechEra, Era>,
}

impl ErasData {
    /// Get an era by its enum variant.
    pub fn get(&self, id: TechEra) -> Option<&Era> {
        self.eras.get(&id)
    }

    /// All loaded eras in canonical order (1 → 5, the order of
    /// [`TechEra::all`]).
    pub fn all_in_order(&self) -> Vec<&Era> {
        TechEra::all()
            .iter()
            .filter_map(|e| self.eras.get(e))
            .collect()
    }
}

/// RON file format for `assets/data/eras.ron`.
///
/// Kept module-private: callers go through [`ErasData`]. The file is a
/// single tuple-struct with one `eras: Vec<Era>` field, mirroring
/// `TechnologiesFile` in `data.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErasFile {
    eras: Vec<Era>,
}

/// Loader system: parse `assets/data/eras.ron` and insert an [`ErasData`]
/// resource. Called by [`crate::data_loader::DataLoaderPlugin`] when
/// `eras.ron` is classified as `LoaderOwned`.
///
/// On parse or IO error, the empty default is inserted and a log line
/// is emitted. The architecture baseline requires the game to boot
/// with empty data; we don't want a malformed `eras.ron` to block the
/// first frame.
pub fn load_eras(mut commands: Commands) {
    info!("Loading era definitions...");

    let path = "assets/data/eras.ron";

    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<ErasFile>(&contents) {
            Ok(data) => {
                let mut eras_data = ErasData::default();
                for era in data.eras {
                    eras_data.eras.insert(era.id, era);
                }
                info!(
                    "Loaded {} era definitions from {}",
                    eras_data.eras.len(),
                    path
                );
                commands.insert_resource(eras_data);
            }
            Err(e) => {
                error!("Failed to parse era data file {}: {}", path, e);
                commands.insert_resource(ErasData::default());
            }
        },
        Err(e) => {
            warn!(
                "Era data file not found at {}: {}. Using empty era set.",
                path, e
            );
            commands.insert_resource(ErasData::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_default_has_no_eras() {
        let data = ErasData::default();
        assert!(data.eras.is_empty());
        assert!(data.all_in_order().is_empty());
    }

    #[test]
    fn get_returns_inserted_era() {
        let mut data = ErasData::default();
        data.eras.insert(
            TechEra::Foundations,
            Era {
                id: TechEra::Foundations,
                description: "Industrial base".to_string(),
                start_year: 0,
                end_year: Some(30),
            },
        );
        let e = data.get(TechEra::Foundations).expect("missing era");
        assert_eq!(e.start_year, 0);
        assert_eq!(e.end_year, Some(30));
    }

    #[test]
    fn all_in_order_skips_missing() {
        let mut data = ErasData::default();
        data.eras.insert(
            TechEra::Propulsion,
            Era {
                id: TechEra::Propulsion,
                description: "Mars belt outer planets".to_string(),
                start_year: 80,
                end_year: Some(250),
            },
        );
        // Only Propulsion is present; ordering should still be canonical.
        let order = data.all_in_order();
        assert_eq!(order.len(), 1);
        assert_eq!(order[0].id, TechEra::Propulsion);
    }

    #[test]
    fn ron_eras_file_round_trip() {
        // A minimal eras.ron with all 5 era variants must parse.
        let ron_src = r#"(
            eras: [
                (id: Foundations, description: "Earth industrial base.", start_year: 0, end_year: Some(30)),
                (id: SpaceOperations, description: "LEO to cislunar.", start_year: 30, end_year: Some(80)),
                (id: Propulsion, description: "Mars, belt, outer planets.", start_year: 80, end_year: Some(250)),
                (id: HabitationColonization, description: "Self-sustaining colonies.", start_year: 250, end_year: Some(400)),
                (id: DefenseIndustry, description: "Star-forts, deep-space industry.", start_year: 400, end_year: None),
            ],
        )"#;
        let parsed: ErasFile = ron::from_str(ron_src).expect("parse eras.ron");
        assert_eq!(parsed.eras.len(), 5);
        assert_eq!(parsed.eras[0].id, TechEra::Foundations);
        assert_eq!(parsed.eras[4].end_year, None);
    }
}
