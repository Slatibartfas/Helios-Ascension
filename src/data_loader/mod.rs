//! Canonical RON data loader.
//!
//! This module is the **first ECS system** built on top of the Bevy 0.18
//! architecture baseline. It is the single, unified entry point for loading
//! RON data files at startup.
//!
//! ## Pipeline
//!
//! ```text
//! assets/data/*.ron
//!   -> scan directory (sorted, deterministic)
//!   -> per-file generic syntax check (parse to `ron::Value`)
//!   -> dispatch to typed loader for known files
//!   -> register typed data as `Resource`s
//!   -> log summary via smoke-check system
//! ```
//!
//! The architecture baseline mandates a single, observable load step before
//! any game logic runs (see `ARCHITECTURE_BASELINE.md` §3 "RON Data Pipeline").
//! All future RON loaders (ship modules, ship hulls, logistics networks, …)
//! should plug in here rather than registering their own `Startup` systems.
//!
//! ## Load strategy choice
//!
//! We use **manual `ron::from_str` + `fs::read_to_string`** at startup, not
//! `bevy::asset::AssetServer`. Rationale:
//!
//! * The RON data is content that exists at compile time and only changes
//!   when the project is rebuilt. We want the data available as
//!   `Resource`s before the first `Update` tick, not as `Handle<T>`s that
//!   resolve asynchronously.
//! * Modders and CI benefit from immediate, hard parse errors at startup
//!   instead of silently-failing asset handles.
//! * This matches the pre-existing convention in `research::data` and
//!   `colony::data`, keeping the diff small for downstream subsystems.
//!
//! ## Adding a new RON data file
//!
//! 1. Drop the file under `assets/data/` (e.g. `ship_modules.ron`).
//! 2. Add a `DataFileKind` variant for it in the [`DataFileKind`] enum below.
//! 3. Add a `dispatch_known_loader` arm that parses it and inserts a typed
//!    `Resource`.
//! 4. If the file is owned by another plugin (e.g. `solar_system.ron` lives
//!    in `plugins::solar_system_data`), set
//!    [`DataFileKind::owned_by_plugin`] and only the syntax check will run
//!    here.
//! 5. Update `DATA_LOADER.md` with the new file's purpose and resource.
//!
//! No RON layout changes are permitted without CTO + LGD joint review
//! (per the architecture baseline).

use bevy::ecs::system::IntoSystem;
use bevy::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

use crate::colony::data::load_buildings;
use crate::research::data::load_technologies;
use crate::research::eras::load_eras;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Canonical data loader plugin. Registers `LoadedDataManifest` and loads
/// every `*.ron` file in `assets/data/` at startup.
pub struct DataLoaderPlugin;

impl Plugin for DataLoaderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedDataManifest>()
            .add_systems(Startup, scan_and_load_data_files)
            .add_systems(PostStartup, log_loaded_data_smoke_check);
    }
}

// ---------------------------------------------------------------------------
// Resource: LoadedDataManifest
// ---------------------------------------------------------------------------

/// Per-file result of the data scan-and-load step.
///
/// One entry per `*.ron` file observed in `assets/data/`. The manifest is
/// the observable surface of the loader: smoke checks, debug overlays, and
/// future modding tools should read from it rather than reaching into
/// per-file resources.
#[derive(Debug, Clone)]
pub struct LoadedDataEntry {
    /// Path relative to the project root (e.g. `"assets/data/buildings.ron"`).
    pub path: String,
    /// File size in bytes, or `None` if the file was missing.
    pub size_bytes: Option<u64>,
    /// How this file is classified in the load pipeline.
    pub kind: DataFileKind,
    /// Outcome of the load attempt.
    pub status: LoadStatus,
    /// Optional human-readable error string when `status` is an error.
    pub error: Option<String>,
}

/// How a given RON file is treated by the loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFileKind {
    /// Loader owned: parses and inserts a typed `Resource` (e.g. buildings).
    LoaderOwned,
    /// Owned by another plugin: syntax check only, no resource insert.
    OwnedByOtherPlugin,
    /// Discovered but no parser wired yet (e.g. future `ship_modules.ron`).
    /// The architecture baseline requires this to be present and parseable
    /// even if the resource is empty.
    AwaitingLoader,
}

/// Outcome of the load attempt for a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    /// File was read and parsed successfully (and a typed resource was
    /// inserted, when `kind == LoaderOwned`).
    Loaded,
    /// File was not present on disk. This is non-fatal: future LGD work
    /// will add the canonical file.
    Missing,
    /// File is on disk but failed to parse (syntax/schema error).
    ParseError,
    /// File is on disk but could not be read (permission / IO error).
    ReadError,
    /// File parsed (syntax OK) but no loader is wired to insert a typed
    /// resource. Use this for files owned by other plugins.
    DelegatedToOtherPlugin,
}

/// Manifest of every RON file observed under `assets/data/` during startup.
///
/// Query this resource to see what loaded, what failed, and what is still
/// waiting for a parser. The smoke-check system in this module logs a
/// summary; UI overlays and modding tooling can read the same data.
#[derive(Resource, Debug, Clone, Default)]
pub struct LoadedDataManifest {
    /// Per-file entries, in sorted (alphabetical) order for determinism.
    pub entries: Vec<LoadedDataEntry>,
}

impl LoadedDataManifest {
    /// Count of files that ended up in the `Loaded` state.
    pub fn loaded_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == LoadStatus::Loaded)
            .count()
    }

    /// Count of files that ended up in any error state.
    pub fn error_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| {
                matches!(
                    e.status,
                    LoadStatus::ParseError | LoadStatus::ReadError
                )
            })
            .count()
    }

    /// Count of files that are present but not yet wired to a typed loader.
    pub fn awaiting_loader_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == DataFileKind::AwaitingLoader)
            .count()
    }

    /// True if any file ended up in a hard error state.
    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }
}

// ---------------------------------------------------------------------------
// File classification
// ---------------------------------------------------------------------------

/// Directory we scan for RON files. Relative to the project root, matches
/// the convention used by every other loader in the codebase.
pub const DATA_DIR: &str = "assets/data";

/// Classify a file by its basename. The basename drives both the kind and
/// the load dispatch — see [`dispatch_known_loader`].
fn classify(basename: &str) -> DataFileKind {
    match basename {
        // Loader-owned: this plugin inserts a typed Resource.
        "buildings.ron" | "technologies.ron" | "eras.ron" => DataFileKind::LoaderOwned,

        // Owned by another plugin: syntax check only, do not double-insert.
        "solar_system.ron" | "planet_textures.ron" => DataFileKind::OwnedByOtherPlugin,

        // Canonical files the architecture baseline expects but for which
        // the typed loader is LGD-scope (see DELA-3 acceptance criteria).
        "ship_modules.ron" | "ship_hulls.ron" => DataFileKind::AwaitingLoader,

        // Any other RON file: assume LGD or modder will wire it later.
        _ => DataFileKind::AwaitingLoader,
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Startup system: scan `assets/data/`, validate each RON file, and dispatch
/// to the per-file typed loader when this plugin owns the file.
fn scan_and_load_data_files(world: &mut World) {
    let mut manifest = world.resource_mut::<LoadedDataManifest>();
    manifest.entries.clear();

    let data_dir = Path::new(DATA_DIR);
    info!(
        target: "data_loader",
        "Scanning RON data directory: {}",
        data_dir.display()
    );

    let mut paths: Vec<PathBuf> = match fs::read_dir(data_dir) {
        Ok(rd) => rd
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("ron"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            warn!(
                target: "data_loader",
                "Data directory {} not readable: {}. All data will load as Missing.",
                data_dir.display(),
                e
            );
            Vec::new()
        }
    };

    // Deterministic ordering: sort by relative path string.
    paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));

    // We must drop the manifest borrow before calling typed loaders, which
    // need `&mut World` themselves to issue `commands.insert_resource(...)`.
    let entries: Vec<LoadedDataEntry> = paths
        .iter()
        .map(|p| build_entry_for_path(p))
        .collect();

    manifest.entries = entries;

    // Drop the borrow on the world so we can mutate again.
    let _ = manifest;

    // Dispatch typed loaders for files this plugin owns.
    for path in &paths {
        let basename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if classify(basename) == DataFileKind::LoaderOwned {
            dispatch_known_loader(world, basename);
        }
    }
}

/// Build a [`LoadedDataEntry`] for one path: read the bytes, run a generic
/// syntax check, and classify the file. The typed-resource insert is handled
/// separately by [`dispatch_known_loader`].
fn build_entry_for_path(path: &Path) -> LoadedDataEntry {
    let path_str = path.to_string_lossy().into_owned();
    let basename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    let kind = classify(&basename);

    let (size_bytes, status, error) = match fs::read_to_string(path) {
        Ok(contents) => {
            let size = contents.len() as u64;
            // Generic syntax check: parse to a generic `ron::Value`. This
            // catches syntax errors, missing commas, unclosed parens, etc.,
            // without committing to a particular schema.
            match ron::from_str::<ron::Value>(&contents) {
                Ok(_) => match kind {
                    DataFileKind::LoaderOwned => (Some(size), LoadStatus::Loaded, None),
                    DataFileKind::OwnedByOtherPlugin => {
                        (Some(size), LoadStatus::DelegatedToOtherPlugin, None)
                    }
                    DataFileKind::AwaitingLoader => (Some(size), LoadStatus::Loaded, None),
                },
                Err(e) => (Some(size), LoadStatus::ParseError, Some(e.to_string())),
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                (None, LoadStatus::Missing, None)
            } else {
                (None, LoadStatus::ReadError, Some(e.to_string()))
            }
        }
    };

    LoadedDataEntry {
        path: path_str,
        size_bytes,
        kind,
        status,
        error,
    }
}

/// Dispatch a per-file typed loader for files this plugin owns.
///
/// We call the existing per-module loaders (`load_technologies`,
/// `load_buildings`) so we don't duplicate parsing logic. The owning
/// modules continue to own the parser types and tests; the plugin is just
/// the canonical scheduling point.
fn dispatch_known_loader(world: &mut World, basename: &str) {
    match basename {
        "technologies.ron" => {
            info!(
                target: "data_loader",
                "Dispatching typed loader: technologies -> TechnologiesData"
            );
            let mut system = IntoSystem::into_system(load_technologies);
            system.initialize(world);
            if let Err(e) = system.run((), world) {
                error!(
                    target: "data_loader",
                    "Typed loader 'load_technologies' failed: {:?}",
                    e
                );
            }
        }
        "buildings.ron" => {
            info!(
                target: "data_loader",
                "Dispatching typed loader: buildings -> BuildingsData"
            );
            let mut system = IntoSystem::into_system(load_buildings);
            system.initialize(world);
            if let Err(e) = system.run((), world) {
                error!(
                    target: "data_loader",
                    "Typed loader 'load_buildings' failed: {:?}",
                    e
                );
            }
        }
        "eras.ron" => {
            info!(
                target: "data_loader",
                "Dispatching typed loader: eras -> ErasData"
            );
            let mut system = IntoSystem::into_system(load_eras);
            system.initialize(world);
            if let Err(e) = system.run((), world) {
                error!(
                    target: "data_loader",
                    "Typed loader 'load_eras' failed: {:?}",
                    e
                );
            }
        }
        _ => {
            // classify() and this match must stay in sync. If you see this
            // branch, add a new arm above.
            warn!(
                target: "data_loader",
                "LoaderOwned file '{}' has no dispatch arm in dispatch_known_loader",
                basename
            );
        }
    }
}

/// PostStartup system: smoke check that at least one typed resource was
/// inserted, and log a one-line summary of the manifest.
///
/// Acceptance criterion: "One Bevy `System` that queries a loaded resource
/// and logs the count."
fn log_loaded_data_smoke_check(
    manifest: Res<LoadedDataManifest>,
    tech_data: Option<Res<crate::research::TechnologiesData>>,
    era_data: Option<Res<crate::research::ErasData>>,
) {
    let tech_count = tech_data
        .as_ref()
        .map(|d| d.technologies.len())
        .unwrap_or(0);
    let era_count = era_data.as_ref().map(|d| d.eras.len()).unwrap_or(0);
    info!(
        target: "data_loader",
        "Smoke check: {} technologies, {} eras loaded from manifest ({} files total, {} errors, {} awaiting loader)",
        tech_count,
        era_count,
        manifest.entries.len(),
        manifest.error_count(),
        manifest.awaiting_loader_count(),
    );

    for entry in &manifest.entries {
        match entry.status {
            LoadStatus::Loaded => {
                info!(
                    target: "data_loader",
                    "  [OK]   {} ({} bytes, {:?})",
                    entry.path,
                    entry.size_bytes.unwrap_or(0),
                    entry.kind,
                );
            }
            LoadStatus::DelegatedToOtherPlugin => {
                info!(
                    target: "data_loader",
                    "  [DELEGATED] {} ({} bytes, owned by other plugin)",
                    entry.path,
                    entry.size_bytes.unwrap_or(0),
                );
            }
            LoadStatus::Missing => {
                warn!(
                    target: "data_loader",
                    "  [MISSING] {} (canonical file not yet present; LGD scope)",
                    entry.path,
                );
            }
            LoadStatus::ParseError => {
                error!(
                    target: "data_loader",
                    "  [PARSE ERROR] {}: {}",
                    entry.path,
                    entry.error.as_deref().unwrap_or("unknown"),
                );
            }
            LoadStatus::ReadError => {
                error!(
                    target: "data_loader",
                    "  [READ ERROR] {}: {}",
                    entry.path,
                    entry.error.as_deref().unwrap_or("unknown"),
                );
            }
        }
    }

    if manifest.has_errors() {
        error!(
            target: "data_loader",
            "Data loader finished with {} error(s); game will run with defaults",
            manifest.error_count(),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_files() {
        assert_eq!(classify("buildings.ron"), DataFileKind::LoaderOwned);
        assert_eq!(classify("technologies.ron"), DataFileKind::LoaderOwned);
        assert_eq!(classify("eras.ron"), DataFileKind::LoaderOwned);
        assert_eq!(
            classify("solar_system.ron"),
            DataFileKind::OwnedByOtherPlugin
        );
        assert_eq!(
            classify("planet_textures.ron"),
            DataFileKind::OwnedByOtherPlugin
        );
        assert_eq!(classify("ship_modules.ron"), DataFileKind::AwaitingLoader);
        assert_eq!(classify("ship_hulls.ron"), DataFileKind::AwaitingLoader);
        assert_eq!(classify("future_thing.ron"), DataFileKind::AwaitingLoader);
    }

    #[test]
    fn manifest_counts_track_entries() {
        let mut m = LoadedDataManifest::default();
        assert_eq!(m.loaded_count(), 0);
        assert_eq!(m.error_count(), 0);
        assert_eq!(m.awaiting_loader_count(), 0);
        assert!(!m.has_errors());

        m.entries.push(LoadedDataEntry {
            path: "a.ron".into(),
            size_bytes: Some(10),
            kind: DataFileKind::LoaderOwned,
            status: LoadStatus::Loaded,
            error: None,
        });
        m.entries.push(LoadedDataEntry {
            path: "b.ron".into(),
            size_bytes: None,
            kind: DataFileKind::AwaitingLoader,
            status: LoadStatus::Missing,
            error: None,
        });
        m.entries.push(LoadedDataEntry {
            path: "c.ron".into(),
            size_bytes: Some(20),
            kind: DataFileKind::AwaitingLoader,
            status: LoadStatus::ParseError,
            error: Some("bad".into()),
        });

        assert_eq!(m.loaded_count(), 1);
        assert_eq!(m.error_count(), 1);
        assert_eq!(m.awaiting_loader_count(), 2);
        assert!(m.has_errors());
    }

    #[test]
    fn all_present_ron_files_parse_as_value() {
        // If any RON file under assets/data/ has a syntax error, this test
        // fails fast and tells the author which one. It does not enforce
        // schema correctness — that's the per-file loader's job.
        let data_dir = Path::new(DATA_DIR);
        let entries: Vec<PathBuf> = match fs::read_dir(data_dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.eq_ignore_ascii_case("ron"))
                        .unwrap_or(false)
                })
                .collect(),
            Err(_) => return, // data dir missing in test env is OK
        };
        for path in entries {
            if let Ok(contents) = fs::read_to_string(&path) {
                ron::from_str::<ron::Value>(&contents)
                    .unwrap_or_else(|e| panic!("RON parse failed for {:?}: {}", path, e));
            }
        }
    }
}
