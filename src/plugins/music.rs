//! Background music playlist system.
//!
//! Plays a looping playlist of ambient tracks during gameplay. Playback
//! controls (play/pause, skip, volume) and the CC-BY attribution are rendered
//! inside the time-controls bottom panel by `ui_time_controls`.
//!
//! ## Adding more tracks
//! Push a new `TrackInfo` entry into the `Vec` inside `MusicPlaylist::default()`.
//! The rest of the system picks it up automatically.

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
    /// Display title used in the attribution overlay.
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
        Self {
            tracks: vec![
                TrackInfo {
                    path: "audio/music/starfire.mp3",
                    title: "Starfire",
                },
                TrackInfo {
                    path: "audio/music/adrift-among-infinite-stars.mp3",
                    title: "Adrift Among Infinite Stars",
                },
                TrackInfo {
                    path: "audio/music/passage-of-time.mp3",
                    title: "Passage Of Time",
                },
            ],
            current_index: 0,
            current_entity: None,
            paused: false,
            volume: 0.6,
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

