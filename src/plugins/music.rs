//! Background music playlist system.
//!
//! Plays a looping playlist of ambient tracks during gameplay and shows a small
//! CC-BY attribution overlay with play/pause, skip, and volume controls so we
//! comply with the Scott Buckley license terms.
//!
//! ## Adding more tracks
//! Push a new `TrackInfo` entry into the `Vec` inside `MusicPlaylist::default()`.
//! The rest of the system picks it up automatically.

use bevy::audio::{AudioPlugin, AudioSink, AudioSinkPlayback, PlaybackMode};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

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
            .add_systems(Update, (advance_playlist, show_music_controls));
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
        .spawn(AudioBundle {
            source: asset_server.load(track.path),
            settings: PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: bevy::audio::Volume::new(playlist.volume),
                paused: playlist.paused,
                ..default()
            },
        })
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
    sinks: Query<&AudioSink>,
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
                commands.entity(e).despawn_recursive();
            }
        }

        playlist.current_index = (playlist.current_index + 1) % playlist.tracks.len();
        spawn_track(&mut commands, &asset_server, &mut playlist);
        return;
    }

    // --- sync pause / volume to live sink ---
    if let Some(entity) = playlist.current_entity {
        if let Ok(sink) = sinks.get(entity) {
            // Sync pause state.
            if playlist.paused && !sink.is_paused() {
                sink.pause();
            } else if !playlist.paused && sink.is_paused() {
                sink.play();
            }
            // Sync volume.
            sink.set_volume(playlist.volume);
        }
    }
}

/// Update: render the music controls bar at the bottom-right corner.
///
/// Shows play/pause, next track, volume slider, then attribution below.
/// `egui::Order::Foreground` ensures this is always drawn on top of all
/// panels (including the time-controls bottom panel, side panels, etc.).
/// The CC-BY 4.0 license requires visible attribution at all times.
fn show_music_controls(mut contexts: EguiContexts, mut playlist: ResMut<MusicPlaylist>) {
    let track_title = playlist.tracks[playlist.current_index].title;

    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    egui::Area::new("music_controls".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(6.0, 3.0))
                .show(ui, |ui| {
                    // Both rows right-aligned (Align::Max in a top-down layout).
                    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                        // Row 1: playback controls
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            ui.spacing_mut().button_padding = egui::vec2(3.0, 1.0);
                            let play_label = if playlist.paused { "▶" } else { "⏸" };
                            if ui.add(egui::Button::new(
                                egui::RichText::new(play_label).small()
                            )).clicked() {
                                playlist.paused = !playlist.paused;
                            }
                            if ui.add(egui::Button::new(
                                egui::RichText::new("⏭").small()
                            )).clicked() {
                                playlist.skip_requested = true;
                            }
                            ui.add_sized(
                                [50.0, 10.0],
                                egui::Slider::new(&mut playlist.volume, 0.0..=1.0)
                                    .show_value(false),
                            );
                        });

                        // Row 2: CC-BY attribution (required by license)
                        ui.label(
                            egui::RichText::new(format!(
                                "♪  '{}' by Scott Buckley — CC-BY 4.0 · www.scottbuckley.com.au",
                                track_title
                            ))
                            .small()
                            .color(egui::Color32::from_rgba_unmultiplied(190, 190, 190, 150)),
                        );
                    });
                });
        });
}

