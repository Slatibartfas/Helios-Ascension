//! `SfxRegistry` (live cue + asset-handle map + cooldown
//! tracker) and `play_sfx_system` (the consumer that turns
//! `Message<SfxEvent>` into `AudioPlayer` entity spawns).
//!
//! Mirrors the spawn pattern in
//! [`crate::plugins::music::spawn_track`]
//! (`src/plugins/music.rs:164-181`) —
//! `AudioPlayer::new(handle) + PlaybackSettings { mode:
//! PlaybackMode::Despawn, volume, paused: false }`. The
//! `Despawn` mode is what lets us ignore the lifecycle
//! entirely: Bevy auto-removes the entity when playback
//! finishes, so the registry never has to track per-cue
//! player entities.
//!
//! ## Cooldown model
//!
//! The cooldown is per-cue (not per-channel) and stored on the
//! registry. On every `play_sfx_system` tick:
//!
//! 1. Drain the `Messages<SfxEvent>` buffer.
//! 2. For each event:
//!    a. Skip if `!registry.ready` (Startup still warming up).
//!    b. Skip if the cue's `cooldown_ms` hasn't elapsed since the last play.
//!    c. Look up the asset handle and the cue metadata.
//!    d. Compute the composed volume via [`SfxBus::volume_for`].
//!    e. Spawn `AudioPlayer + PlaybackSettings::Despawn`.
//!    f. Stamp `registry.cooldowns[id]` = current instant.
//!
//! ## Edge cases
//!
//! - **Rapid duplicate events in one frame**: deduped by the
//!   cooldown (the second event sees the cooldown stamp set by
//!   the first in the same iteration).
//! - **Cue with `cooldown_ms: 0`**: every event plays. Used
//!   for cues where saturation isn't a concern.
//! - **Missing cue in registry**: skip + `debug!` (modder
//!   authored a manifest entry without adding the matching
//!   Rust variant, or load failed silently).
//! - **Asset still loading**: skip + `debug!` (the manifest
//!   loader is synchronous but the asset handle's load state
//!   can still be `Loading` at first play).

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings};
use bevy::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

use super::{clamped_default_volume, cooldown_duration, linear_volume, SfxBus, SfxCueId, SfxEvent};

/// Live registry — built by [`super::manifest::load_sfx_manifest`]
/// from the on-disk manifest, then read by [`play_sfx_system`].
#[derive(Resource, Debug, Default)]
pub struct SfxRegistry {
    /// Cue metadata, keyed by stable id.
    pub(crate) cues: HashMap<SfxCueId, super::SfxCue>,
    /// Resolved asset handles, keyed by stable id.
    pub(crate) assets: HashMap<SfxCueId, Handle<AudioSource>>,
    /// Per-cue "last played at" instant (using
    /// [`std::time::Instant`] for monotonic time). The cooldown
    /// math compares against `Instant::now()` — see
    /// [`play_sfx_system`].
    pub(crate) cooldowns: HashMap<SfxCueId, Instant>,
    /// Last seen manifest id string per cue. Used to detect
    /// drift between Rust enum and manifest (the audit script
    /// does the same check in CI; this is a runtime safety net).
    pub(crate) last_seen_manifest_id: HashMap<SfxCueId, String>,
    /// `true` once the manifest has been resolved (success,
    /// failure, or empty).
    pub(crate) ready: bool,
}

/// `Update` system — consume `Messages<SfxEvent>` and spawn one
/// `AudioPlayer` per accepted event.
///
/// Runs last in the SFX chain (after the bus sync + the two
/// bridges) so the volume it computes reflects the latest
/// settings and the events are already populated.
pub fn play_sfx_system(
    mut commands: Commands,
    mut events: MessageReader<SfxEvent>,
    mut registry: ResMut<SfxRegistry>,
    bus: Res<SfxBus>,
) {
    if !registry.ready {
        // Registry not warmed yet — silently drop. The manifest
        // loader flips `ready` synchronously on Startup; if it's
        // false at the first Update tick, the load failed and we
        // don't want to queue plays into a broken registry.
        events.clear();
        return;
    }

    let now = Instant::now();
    let mut accepted = 0usize;
    let mut cooldown_blocked = 0usize;
    let mut missing_cue = 0usize;

    for SfxEvent(id) in events.read() {
        let Some(cue) = registry.cues.get(id) else {
            missing_cue += 1;
            debug!("sfx: requested cue {id:?} not in registry (manifest drift?)");
            continue;
        };
        let Some(handle) = registry.assets.get(id) else {
            missing_cue += 1;
            debug!("sfx: requested cue {id:?} has no asset handle (load failed?)");
            continue;
        };

        // Cooldown gate — same-frame duplicates are deduped by
        // stamping the cooldown before spawning.
        let cd = cooldown_duration(cue);
        if !cd.is_zero() {
            if let Some(last) = registry.cooldowns.get(id) {
                if now.duration_since(*last) < cd {
                    cooldown_blocked += 1;
                    continue;
                }
            }
        }

        // Compute the composed volume. `SfxBus::volume_for`
        // already clamps to [0, 1].
        let v = bus.volume_for(cue.category, clamped_default_volume(cue));
        if v <= 0.0 {
            // Master muted or category muted — skip the spawn
            // entirely so we don't queue a silent audio buffer.
            continue;
        }

        // Spawn the AudioPlayer. `PlaybackMode::Despawn` means
        // Bevy removes the entity when playback finishes, so we
        // never have to track the player lifecycle.
        commands.spawn((
            AudioPlayer::new(handle.clone()),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: linear_volume(v),
                paused: false,
                ..default()
            },
        ));

        // Stamp the cooldown AFTER spawning so a rapid second
        // event in the same drain loop sees the new instant and
        // gets blocked.
        registry.cooldowns.insert(*id, now);
        accepted += 1;
    }

    if accepted > 0 || cooldown_blocked > 0 || missing_cue > 0 {
        debug!(
            "sfx: drain accepted={accepted} cooldown_blocked={cooldown_blocked} \
             missing_cue={missing_cue}"
        );
    }
}
