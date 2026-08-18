//! Background music playlist system.
//!
//! Plays a looping playlist of ambient tracks during gameplay. Playback
//! controls (play/pause, skip, volume) are rendered inside the time-controls
//! bottom panel by `ui_time_controls`. The currently-playing track title
//! is shown next to the controls; there is no attribution overlay — all
//! tracks in the playlist are AI-generated (MiniMax Music 3.0), and the
//! exact prompt used to produce each one is logged in
//! `assets/data/music_prompts.ron` for reproducibility and audit.
//!
//! ## Adding more tracks
//! Push a new [`TrackInfo`] entry into the `Vec` inside
//! `MusicPlaylist::default()`. The rest of the system picks it up
//! automatically; no other code change is required.
//!
//! ## Volume policy
//! [`MusicPlaylist::volume`] defaults to `0.4` so the playlist starts
//! quiet on first launch. The in-game slider (bottom-right of the time
//! controls) lets the player raise it back up; the value persists in
//! `MusicPlaylist` for the lifetime of the app and is not stored to
//! disk — the initial volume is always the default.

use bevy::audio::{AudioPlugin, AudioSink, AudioSinkPlayback, PlaybackMode};
use bevy::prelude::*;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        // Ensure Bevy's audio plugin is present (it normally is via DefaultPlugins).
        if !app.is_plugin_added::<AudioPlugin>() {
            app.add_plugins(AudioPlugin::default());
        }

        app.init_resource::<MusicPlaylist>()
            .add_systems(Startup, start_playlist)
            .add_systems(Update, advance_playlist);
    }
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// Metadata for one track in the playlist.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    /// Asset path relative to the `assets/` directory.
    pub path: &'static str,
    /// Display title used in the bottom-right track display.
    pub title: &'static str,
}

/// Global playlist state resource.
#[derive(Resource)]
pub struct MusicPlaylist {
    pub tracks: Vec<TrackInfo>,
    /// Index of the track that is currently (or was last) started.
    pub current_index: usize,
    /// The entity that owns the current `AudioSink`. `None` before the first
    /// track is spawned, or after the entity has been despawned by Bevy.
    pub current_entity: Option<Entity>,
    /// Whether playback is paused.
    pub paused: bool,
    /// Master volume, 0.0–1.0.
    pub volume: f32,
    /// Set to true by the UI when the user wants to skip to the next track.
    pub skip_requested: bool,
}

impl Default for MusicPlaylist {
    fn default() -> Self {
        // Ten AI-generated ambient tracks (MiniMax Music 3.0), interleaved
        // so the playlist flows from vast and slow to brighter and back
        // down again. Prompts and generation parameters are recorded in
        // `assets/data/music_prompts.ron` so each track is reproducible.
        // All ten tracks use the same cinematic-space-orchestral template
        // (gentle piano + soft synth pads + distant strings; no vocals;
        // 75-100 BPM; mention "4X space strategy game" in the prompt) so
        // the playlist reads as one cohesive score.
        //
        // Anti-vocal stack: every prompt is a short ambient brief (mood +
        // 2-3 instrument cues + use case + BPM) with the literal "no vocals"
        // clause. We deliberately do NOT add Stellaris / Homeworld /
        // Terra Invicta / Mass Effect style references — those are OST
        // references and tip the model into "song with vocals" mode even
        // when the prompt explicitly forbids it. We also do NOT add
        // song-structure tags ("intro-verse-chorus-verse-outro"); "chorus"
        // especially nudges the model toward vocal hooks. The matching
        // `mmx music generate` invocations also pass `--instrumental` and
        // `--avoid "vocals, choir, singing, vocal pads, harmony, voice"`
        // for a second anti-vocal layer.
        // Order (BPM / key):
        //   1. Helios Magnetosphere      (75 / A minor)   vast, expansive
        //   2. Helios Drift              (80 / D minor)   contemplative
        //   3. Helios Signal in the Dark (85 / C minor)   mysterious
        //   4. Helios Jovian Rendezvous  (90 / C minor)   building tension
        //   5. Helios Subsurface         (80 / D minor)   mining
        //   6. Helios New Horizons       (95 / G major)   discovery
        //   7. Helios Decay Orbit        (78 / E minor)   bittersweet
        //   8. Helios First Light        (100 / E major)  uplifting
        //   9. Helios Prograde Burn      (95 / G major)   forward momentum
        //  10. Helios Long Vigil         (75 / A minor)   late-game
        Self {
            tracks: vec![
                TrackInfo {
                    path: "audio/music/helios-magnetosphere.mp3",
                    title: "Helios Magnetosphere",
                },
                TrackInfo {
                    path: "audio/music/helios-drift.mp3",
                    title: "Helios Drift",
                },
                TrackInfo {
                    path: "audio/music/helios-signal-in-the-dark.mp3",
                    title: "Helios Signal in the Dark",
                },
                TrackInfo {
                    path: "audio/music/helios-jovian-rendezvous.mp3",
                    title: "Helios Jovian Rendezvous",
                },
                TrackInfo {
                    path: "audio/music/helios-subsurface.mp3",
                    title: "Helios Subsurface",
                },
                TrackInfo {
                    path: "audio/music/helios-new-horizons.mp3",
                    title: "Helios New Horizons",
                },
                TrackInfo {
                    path: "audio/music/helios-decay-orbit.mp3",
                    title: "Helios Decay Orbit",
                },
                TrackInfo {
                    path: "audio/music/helios-first-light.mp3",
                    title: "Helios First Light",
                },
                TrackInfo {
                    path: "audio/music/helios-prograde-burn.mp3",
                    title: "Helios Prograde Burn",
                },
                TrackInfo {
                    path: "audio/music/helios-long-vigil.mp3",
                    title: "Helios Long Vigil",
                },
            ],
            current_index: 0,
            current_entity: None,
            paused: false,
            volume: 0.4,
            skip_requested: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spawn a new audio entity for the track at `playlist.current_index`.
/// `PlaybackMode::Despawn` means Bevy automatically removes the entity when
/// playback finishes, which we use in `advance_playlist` to detect completion.
fn spawn_track(commands: &mut Commands, asset_server: &AssetServer, playlist: &mut MusicPlaylist) {
    let track = &playlist.tracks[playlist.current_index];
    let entity = commands
        .spawn((
            AudioPlayer::new(asset_server.load(track.path)),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: bevy::audio::Volume::Linear(playlist.volume),
                paused: playlist.paused,
                ..default()
            },
        ))
        .id();
    playlist.current_entity = Some(entity);
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Startup: begin playing the first track immediately.
fn start_playlist(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut playlist: ResMut<MusicPlaylist>,
) {
    spawn_track(&mut commands, &asset_server, &mut playlist);
}

/// Update: advance to next track when the current one ends (entity despawned),
/// handle skip requests, and sync volume/pause state to the live `AudioSink`.
fn advance_playlist(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut playlist: ResMut<MusicPlaylist>,
    mut sinks: Query<&mut AudioSink>,
    entities: Query<Entity>,
) {
    // --- skip or natural end ---
    let track_ended = playlist
        .current_entity
        .map(|e| entities.get(e).is_err())
        .unwrap_or(false);

    if track_ended || playlist.skip_requested {
        playlist.skip_requested = false;

        // Despawn current entity if skipping mid-track.
        if let Some(e) = playlist.current_entity.take() {
            if entities.get(e).is_ok() {
                commands.entity(e).despawn();
            }
        }

        playlist.current_index = (playlist.current_index + 1) % playlist.tracks.len();
        spawn_track(&mut commands, &asset_server, &mut playlist);
        return;
    }

    // --- sync pause / volume to live sink ---
    if let Some(entity) = playlist.current_entity {
        if let Ok(mut sink) = sinks.get_mut(entity) {
            // Sync pause state.
            if playlist.paused && !sink.is_paused() {
                sink.pause();
            } else if !playlist.paused && sink.is_paused() {
                sink.play();
            }
            // Sync volume.
            sink.set_volume(bevy::audio::Volume::Linear(playlist.volume));
        }
    }
}
