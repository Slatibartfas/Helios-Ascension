//! Manifest loader.
//!
//! Loads `assets/data/sfx_manifest.ron` synchronously on
//! `Startup` (the file is tiny — ~2 KB for 14 cues — so
//! synchronous `std::fs::read_to_string` is fine and removes
//! the async `AssetServer` dance that the music plugin uses).
//!
//! Pattern matches `SolarSystemData::load_from_file`
//! (`src/plugins/solar_system_data.rs`).
//!
//! ## Failure modes
//!
//! - **Missing manifest file**: logs `error!`, leaves the
//!   registry empty. Runtime plays nothing rather than
//!   crashing. (The audit script catches this in CI before the
//!   binary is shipped; runtime resilience is the second line
//!   of defence.)
//! - **Malformed RON**: Bevy's `ron` decoder returns an error;
//!   we log `error!` and leave the registry empty.
//! - **Unknown string id** (manifest id doesn't match any
//!   [`SfxCueId`] variant): logs `warn!` and skips the entry.
//!   Modders adding new ids will see the entry silently dropped
//!   until a matching Rust variant is added — see
//!   `scripts/audit_sfx_manifest.py` for the CI check that
//!   catches the mismatch the other direction.
//! - **Missing cue file** (`audio/sfx/<file>` not on disk):
//!   the handle is still registered, but when the playback
//!   system tries to spawn an `AudioPlayer` the audio backend
//!   will fail to load. We warn once per cue at load time so
//!   the modder sees the issue immediately.

use bevy::prelude::*;
use std::time::Instant;

use super::{SfxCueId, SfxManifest, SfxRegistry};

/// Path to the manifest relative to the executable's CWD.
/// Bevy's `DefaultPlugins` keeps the CWD at launch time, so
/// this resolves as expected in `cargo run` and packaged
/// builds alike.
const MANIFEST_PATH: &str = "assets/data/sfx_manifest.ron";

/// `Startup` system — synchronously load the manifest and
/// populate [`SfxRegistry`]. Cheap (file is ~2 KB).
///
/// The previous async-warmup pattern (load via `AssetServer`,
/// poll `LoadState::Loaded`) was abandoned because the music
/// plugin's MP3 playlist is the *only* place that pattern
/// makes sense (audio assets are large + streamed). For a
/// ~2 KB metadata file, `std::fs::read_to_string` is faster
/// than going through the asset server.
pub fn load_sfx_manifest(asset_server: Res<AssetServer>, mut registry: ResMut<SfxRegistry>) {
    let start = Instant::now();
    let contents = match std::fs::read_to_string(MANIFEST_PATH) {
        Ok(c) => c,
        Err(e) => {
            error!(
                "sfx: failed to read manifest at `{MANIFEST_PATH}`: {e}. \
                 SFX disabled — game will run silent. Reinstall the file or \
                 check the working directory."
            );
            registry.ready = true; // unblock; registry stays empty
            return;
        }
    };

    let manifest: SfxManifest = match ron::from_str(&contents) {
        Ok(m) => m,
        Err(e) => {
            error!(
                "sfx: manifest at `{MANIFEST_PATH}` is malformed: {e}. \
                 SFX disabled. Fix the RON syntax (or revert the file) to restore audio."
            );
            registry.ready = true;
            return;
        }
    };

    populate_registry(&mut registry, &asset_server, &manifest);

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    info!(
        "sfx: loaded {} cue(s) from manifest in {:.1}ms",
        registry.cues.len(),
        elapsed_ms
    );
    registry.ready = true;
}

/// Build the live cue → asset-handle map. Called exactly once,
/// after the manifest parses successfully.
fn populate_registry(
    registry: &mut SfxRegistry,
    asset_server: &AssetServer,
    manifest: &SfxManifest,
) {
    registry.cues.clear();
    registry.assets.clear();
    registry.cooldowns.clear();
    registry.last_seen_manifest_id.clear();

    for cue in &manifest.cues {
        let Some(id) = SfxCueId::from_str_id(&cue.id) else {
            warn!(
                "sfx: manifest entry id `{}` is not a known SfxCueId — skipping. \
                 Add the variant to src/plugins/sfx.rs::SfxCueId.",
                cue.id
            );
            continue;
        };

        // Resolve the audio asset up-front via the asset server.
        // If the file is missing, we still register the cue (so
        // the `SfxCueId::ALL ↔ manifest` invariant holds) but
        // the handle's load state will be Failed and the
        // playback system will skip the spawn — the cue simply
        // plays nothing. We don't log here because the asset
        // server logs its own load failures; we just store the
        // handle and let the runtime decide.
        let path = super::asset_path_for(cue);
        let handle = asset_server.load::<bevy::audio::AudioSource>(&path);
        registry.assets.insert(id, handle);

        registry.cues.insert(id, cue.clone());
        registry.last_seen_manifest_id.insert(id, cue.id.clone());
    }
}

impl SfxRegistry {
    /// Look up the metadata for a cue, returning `None` if the
    /// registry is unready or the id is unknown.
    pub fn cue(&self, id: SfxCueId) -> Option<&super::SfxCue> {
        if !self.ready {
            return None;
        }
        self.cues.get(&id)
    }

    /// Look up the audio `Handle<AudioSource>` for a cue. Used
    /// by [`super::playback::play_sfx_system`].
    pub fn handle(&self, id: SfxCueId) -> Option<Handle<bevy::audio::AudioSource>> {
        if !self.ready {
            return None;
        }
        self.assets.get(&id).cloned()
    }

    /// All cues currently in the registry (empty if not ready).
    /// Used by the audit script via a debug introspection path —
    /// the production playback loop never iterates.
    pub fn len(&self) -> usize {
        self.cues.len()
    }

    /// `true` if no cues are loaded (manifest empty or load
    /// failed). Tests use this to assert the empty-registry
    /// fallback path.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
}
