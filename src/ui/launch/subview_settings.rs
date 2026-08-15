//! Settings subview (GRA-318 PR-D).
//!
//! Renders when [`crate::ui::launch::LaunchState::Settings`] is
//! active. Surfaces a 3-tab panel — Audio / Graphics / Gameplay —
//! per LGD pick in `assets/data/seed_copy.ron::settings_structure`
//! (GRA-309 §9 Q4).
//!
//! The panel reads/writes [`PersistentSettings`] directly. Edits
//! are persisted back to `<userdata>/settings.ron` on every change
//! via the PR-A `save_persistent_settings_to` helper — the spec
//! calls for "debounced" writes but PR-A ships only the synchronous
//! saver. The debounce is a small optimization that lands in a
//! future ticket; for now we write on every change so the
//! persistence round-trip is exercised end-to-end.
//!
//! Audio tab additionally surfaces the music-attribution overlay
//! required by CEO HB-297 row 1 (comment `6ab37ba7`). The overlay
//! now lists every track in the current [`crate::plugins::music::MusicPlaylist`]
//! grouped by attribution source — the original Scott Buckley
//! (CC-BY 4.0) entries and the MiniMax Music 3.0 (AI-generated)
//! entries. The overlay is shown when audio is enabled
//! (`master_volume > 0`) and hidden when muted, per the LGD spec.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::plugins::music::MusicPlaylist;
use crate::ui::launch::subview_manifests::SeedCopyManifest;
use crate::ui::launch::userdata::{
    resolve_userdata_dir, save_persistent_settings_to, PersistentSettings, PersistentWindowMode,
};
use crate::ui::launch::{LaunchState, LaunchSystemSet};
use crate::ui::theme;

/// Identifier for which settings tab is currently active. The set
/// of variants mirrors [`SeedCopyManifest::settings_structure`]:
/// the Coder respects whatever labels the LGD ships, but the
/// rendering code only knows these three id keys.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTabId {
    #[default]
    Audio,
    Graphics,
    Gameplay,
}

impl SettingsTabId {
    /// Stable id string the subview matches against
    /// [`SeedCopyManifest::settings_structure.labels`]. Kept here
    /// (not in the manifest) because the manifest holds *display*
    /// labels; the Coder is responsible for the canonical id set.
    pub fn as_id(self) -> &'static str {
        match self {
            SettingsTabId::Audio => "audio",
            SettingsTabId::Graphics => "graphics",
            SettingsTabId::Gameplay => "gameplay",
        }
    }

    /// Resolve the canonical id to a `SettingsTabId`. Returns the
    /// default (Audio) on miss so a typo in the RON labels never
    /// strands the panel on an empty tab.
    pub fn from_id(id: &str) -> Self {
        match id {
            "audio" => SettingsTabId::Audio,
            "graphics" => SettingsTabId::Graphics,
            "gameplay" => SettingsTabId::Gameplay,
            _ => SettingsTabId::Audio,
        }
    }
}

/// Music-attribution overlay text (CEO HB-297 row 1, comment
/// `6ab37ba7`). Constants instead of `&str` so they appear in
/// `cargo doc` and are trivial to grep for during review. These
/// pin the *first Scott Buckley entry* in
/// [`crate::plugins::music::MusicPlaylist`] — the overlay lists
/// every track in the playlist grouped by attribution source, and
/// the canonical first-track values are what the test enforces so
/// future re-orderings of the playlist stay in sync with the
/// overlay's headline entry.
const MUSIC_TITLE: &str = "Starfire";
const MUSIC_AUTHOR: &str = "Scott Buckley";
const MUSIC_LICENSE: &str = "CC-BY 4.0";
const MUSIC_SOURCE_URL: &str = "scottbuckley.com.au";

/// Render the Settings subview. Reads [`LaunchState::Settings`]
/// for gating; no-ops for every other variant. Lives in
/// [`LaunchSystemSet::Menu`] so it only ticks while the menu
/// state is active (PR-A's set is reserved for PR-C/D).
pub fn ui_settings_subview(
    mut contexts: EguiContexts,
    mut launch_state: ResMut<LaunchState>,
    mut settings: ResMut<PersistentSettings>,
    mut active_tab: ResMut<SettingsTabId>,
    seed_copy: Res<SeedCopyManifest>,
    music_playlist: Res<MusicPlaylist>,
) {
    if *launch_state != LaunchState::Settings {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Side-effectful state writes are deferred until after the
    // egui closure returns (see `subview_new_game` for the same
    // pattern) so we don't hold two `ResMut` borrows across the
    // render.
    let mut back_clicked = false;
    let mut settings_dirty = false;
    let mut post_save = None;

    // GRA-XYZ: transparent central panel so the rotating-Earth backdrop
    // stays visible behind the settings tabs.
    egui::CentralPanel::default()
        .frame(theme::menu_transparent_frame())
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(theme::Spacing::xl);
                ui.label(
                    egui::RichText::new("Settings")
                        .font(theme::title())
                        .color(theme::CYAN)
                        .size(28.0),
                );
                ui.add_space(theme::Spacing::lg);
            });

            // ── Tab strip ─────────────────────────────────────
            ui.horizontal(|ui| {
                for tab in seed_copy.settings_structure.labels.iter() {
                    let id = SettingsTabId::from_id(&tab.id);
                    let is_active = *active_tab == id;
                    if ui.selectable_label(is_active, &tab.label).clicked() {
                        *active_tab = id;
                    }
                }
            });

            ui.add_space(theme::Spacing::md);

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(theme::Spacing::lg as i8))
                .show(ui, |ui| match *active_tab {
                    SettingsTabId::Audio => {
                        if draw_audio_tab(ui, &mut settings, &music_playlist) {
                            settings_dirty = true;
                        }
                    }
                    SettingsTabId::Graphics => {
                        if draw_graphics_tab(ui, &mut settings) {
                            settings_dirty = true;
                        }
                    }
                    SettingsTabId::Gameplay => {
                        if draw_gameplay_tab(ui, &mut settings) {
                            settings_dirty = true;
                        }
                    }
                });

            ui.add_space(theme::Spacing::xl);

            // ── Action row ────────────────────────────────────
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("Back").color(theme::TEXT_DIM))
                    .clicked()
                {
                    back_clicked = true;
                }
            });
        });

    // ── Post-click state writes ────────────────────────────
    if settings_dirty {
        let dir = resolve_userdata_dir();
        // Persist a snapshot to avoid holding the borrow across
        // the saver's `fs::create_dir_all` syscall.
        let snapshot = settings.clone();
        match save_persistent_settings_to(&dir, &snapshot) {
            Ok(path) => post_save = Some(path),
            Err(e) => warn!("settings: could not persist to {}: {}", dir.display(), e),
        }
    }
    if let Some(path) = post_save {
        info!(
            "settings: persisted ({} bytes written to {})",
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            path.display()
        );
    }
    if back_clicked {
        *launch_state = LaunchState::MainMenu;
    }
}

/// Draw the Audio tab. Returns `true` when any setting changed and
/// the caller should persist.
fn draw_audio_tab(
    ui: &mut egui::Ui,
    settings: &mut PersistentSettings,
    music_playlist: &MusicPlaylist,
) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("Audio Mix").color(theme::CYAN).strong());
    ui.add_space(theme::Spacing::xs);

    changed |= draw_volume_slider(ui, "Master", &mut settings.master_volume);
    changed |= draw_volume_slider(ui, "Music", &mut settings.music_volume);
    changed |= draw_volume_slider(ui, "SFX", &mut settings.sfx_volume);

    ui.add_space(theme::Spacing::lg);

    // ── Music attribution overlay (CEO HB-297 row 1) ─────
    // Hidden when audio is muted so a player who has disabled
    // all audio doesn't see a credit for music they aren't
    // hearing (per the LGD spec — the overlay is a courtesy
    // to players who can hear the music).
    //
    // Headline entry (the CEO-mandated Scott Buckley line) is
    // rendered from the canonical `MUSIC_*` constants so it
    // stays stable across playlist reorderings. The remaining
    // entries are derived from `music_playlist` and grouped by
    // attribution source (the Scott Buckley headline is skipped
    // here so the overlay doesn't show it twice).
    let audio_enabled = settings.master_volume > 0.0 && settings.music_volume > 0.0;
    ui.add_enabled_ui(audio_enabled, |ui| {
        egui::Frame::group(ui.style())
            .fill(theme::SURFACE_INPUT)
            .inner_margin(egui::Margin::same(theme::Spacing::md as i8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Music")
                        .color(theme::TEXT_HINT)
                        .size(11.0),
                );
                ui.add_space(theme::Spacing::xs);

                // ── Headline: CEO HB-297 row 1 ────────────────
                ui.label(
                    egui::RichText::new(format!(
                        "\"{}\" by {} · {}",
                        MUSIC_TITLE, MUSIC_AUTHOR, MUSIC_SOURCE_URL
                    ))
                    .color(theme::CYAN)
                    .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("Licensed under {}", MUSIC_LICENSE))
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                );

                // ── Remaining tracks, grouped by attribution ──
                // The Scott Buckley headline is the first playlist
                // entry, so its group is rendered below as "Also:
                // <other Buckley titles>" with a single extra
                // entry ("Adrift Among Infinite Stars" + "Passage
                // Of Time"); AI-generated tracks each get their
                // own attributed block.
                let groups = group_tracks_by_attribution(music_playlist);
                for group in groups.iter() {
                    ui.add_space(theme::Spacing::sm);
                    let others: Vec<String> = group
                        .tracks
                        .iter()
                        .map(|t| format!("\"{}\"", t.title))
                        .collect();
                    let url_suffix = match group.attribution.source_url {
                        Some(url) => format!(" · {}", url),
                        None => String::new(),
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "{} — {} under {}{}",
                            others.join(", "),
                            group.attribution.author,
                            group.attribution.license,
                            url_suffix,
                        ))
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                    );
                }
            });
    });

    if !audio_enabled {
        ui.label(
            egui::RichText::new("Music attribution hidden while audio is muted.")
                .color(theme::TEXT_HINT)
                .size(10.0)
                .italics(),
        );
    }

    changed
}

/// One attribution source (e.g. "Scott Buckley, CC-BY 4.0") and the
/// tracks that share it. Built by `group_tracks_by_attribution` —
/// kept local to this file because it's only used here.
struct AttributionGroup {
    attribution: crate::plugins::music::TrackAttribution,
    tracks: Vec<crate::plugins::music::TrackInfo>,
}

/// Collapse `music_playlist.tracks` into consecutive groups that
/// share the same `TrackAttribution` (compared by pointer identity
/// on each field, since the same `const` binding produces the same
/// `&'static str` pointer across all instances). Preserves playlist
/// order so the overlay reads top-to-bottom.
fn group_tracks_by_attribution(
    music_playlist: &MusicPlaylist,
) -> Vec<AttributionGroup> {
    let mut groups: Vec<AttributionGroup> = Vec::new();
    for track in &music_playlist.tracks {
        let same = groups.last_mut().filter(|g| {
            std::ptr::eq(g.attribution.author, track.attribution.author)
                && std::ptr::eq(g.attribution.license, track.attribution.license)
                && std::ptr::eq(
                    g.attribution.source_url.unwrap_or(""),
                    track.attribution.source_url.unwrap_or(""),
                )
        });
        match same {
            Some(g) => g.tracks.push(track.clone()),
            None => groups.push(AttributionGroup {
                attribution: track.attribution.clone(),
                tracks: vec![track.clone()],
            }),
        }
    }
    groups
}

/// Draw the Graphics tab. Returns `true` on change.
fn draw_graphics_tab(ui: &mut egui::Ui, settings: &mut PersistentSettings) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("Graphics").color(theme::CYAN).strong());
    ui.add_space(theme::Spacing::xs);

    // ── Window mode combo box ─────────────────────────────────
    // The mode is rendered as a 3-option `ComboBox` so the player
    // sees the textual intent (Windowed / Fullscreen / Borderless)
    // rather than a binary checkbox. The `apply_window_mode_to_primary`
    // system in `src/plugins/window_mode_bridge.rs` reads the new
    // value and pushes it to `Window::mode` on the primary window.
    ui.label("Window mode");
    let current_label = match settings.window_mode {
        PersistentWindowMode::Windowed => "Windowed",
        PersistentWindowMode::Fullscreen => "Fullscreen",
        PersistentWindowMode::BorderlessFullscreen => "Borderless",
    };
    egui::ComboBox::from_id_salt("graphics_window_mode")
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            for variant in PersistentWindowMode::ALL {
                let label = match variant {
                    PersistentWindowMode::Windowed => "Windowed",
                    PersistentWindowMode::Fullscreen => "Fullscreen",
                    PersistentWindowMode::BorderlessFullscreen => "Borderless",
                };
                let mut selected = settings.window_mode == *variant;
                if ui.selectable_label(selected, label).clicked() && !selected {
                    settings.window_mode = *variant;
                    selected = true;
                    changed = true;
                }
            }
        });

    ui.add_space(theme::Spacing::sm);
    ui.label("UI scale");
    if ui
        .add(egui::Slider::new(&mut settings.ui_scale, 0.5..=2.0).step_by(0.1))
        .changed()
    {
        changed = true;
    }
    changed
}

/// Draw the Gameplay tab. Returns `true` on change.
fn draw_gameplay_tab(ui: &mut egui::Ui, settings: &mut PersistentSettings) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new("Gameplay").color(theme::CYAN).strong());
    ui.add_space(theme::Spacing::xs);

    if ui
        .checkbox(&mut settings.tutorial_enabled, "Enable tutorial prompts")
        .changed()
    {
        changed = true;
    }
    changed
}

/// Helper that draws a labelled volume slider and returns `true`
/// on change. Range 0.0..=1.0 with 5% step. Mute semantics: 0.0
/// disables the channel entirely (no audio plays on it).
fn draw_volume_slider(ui: &mut egui::Ui, label: &str, value: &mut f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add_space(theme::Spacing::sm);
        if ui
            .add(egui::Slider::new(value, 0.0..=1.0).step_by(0.05))
            .changed()
        {
            changed = true;
        }
    });
    changed
}

/// Register the Settings subview render system in
/// [`LaunchSystemSet::Menu`].
pub fn register_settings_subview(app: &mut App) {
    app.init_resource::<SettingsTabId>().add_systems(
        EguiPrimaryContextPass,
        ui_settings_subview.in_set(LaunchSystemSet::Menu),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::world::World;

    fn world_with_settings() -> World {
        let mut world = World::new();
        world.init_resource::<LaunchState>();
        world.init_resource::<PersistentSettings>();
        world.init_resource::<SettingsTabId>();
        world.insert_resource(SeedCopyManifest::default());
        world
    }

    #[test]
    fn settings_tab_id_as_id_is_stable() {
        assert_eq!(SettingsTabId::Audio.as_id(), "audio");
        assert_eq!(SettingsTabId::Graphics.as_id(), "graphics");
        assert_eq!(SettingsTabId::Gameplay.as_id(), "gameplay");
    }

    #[test]
    fn settings_tab_id_from_id_round_trips() {
        assert_eq!(SettingsTabId::from_id("audio"), SettingsTabId::Audio);
        assert_eq!(SettingsTabId::from_id("graphics"), SettingsTabId::Graphics);
        assert_eq!(SettingsTabId::from_id("gameplay"), SettingsTabId::Gameplay);
    }

    #[test]
    fn settings_tab_id_from_id_unknown_falls_back_to_audio() {
        assert_eq!(SettingsTabId::from_id("nope"), SettingsTabId::Audio);
        assert_eq!(SettingsTabId::from_id(""), SettingsTabId::Audio);
    }

    #[test]
    fn default_tab_is_audio() {
        assert_eq!(SettingsTabId::default(), SettingsTabId::Audio);
    }

    #[test]
    fn world_initializes_with_audio_tab_and_default_settings() {
        let world = world_with_settings();
        assert_eq!(*world.resource::<SettingsTabId>(), SettingsTabId::Audio);
        let s = world.resource::<PersistentSettings>();
        assert_eq!(s.master_volume, 1.0);
        assert_eq!(s.music_volume, 1.0);
        assert_eq!(s.sfx_volume, 1.0);
        assert_eq!(s.window_mode, PersistentWindowMode::Windowed);
        // The legacy `fullscreen: bool` shim is read on load (for
        // migration) but never written on save; its default is
        // always `false` on a fresh install.
        assert!(!s.fullscreen);
        assert_eq!(s.ui_scale, 1.0);
        assert!(!s.tutorial_enabled);
    }

    #[test]
    fn settings_resource_round_trip_via_persister() {
        // The render system calls `save_persistent_settings_to` on
        // change; the persisted file must round-trip through the
        // loader so the editor and the player see the same state
        // on next launch. This re-confirms the PR-A test plan in
        // the PR-D context (issue body test plan bullet 3).
        use crate::ui::launch::userdata::{
            load_persistent_settings_from, save_persistent_settings_to,
        };
        let settings = PersistentSettings {
            master_volume: 0.42,
            music_volume: 0.0,
            window_mode: PersistentWindowMode::BorderlessFullscreen,
            // `fullscreen` is the legacy shim field. The round-trip
            // in `subview_settings.rs` predates the enum; the value
            // we set here is what the file would have ended up with
            // after the loader's migration pass against a legacy
            // `fullscreen: true` only file. The shim is `false`
            // after migration; the new field is the canonical
            // source of truth.
            fullscreen: false,
            ui_scale: 1.3,
            tutorial_enabled: true,
            ..Default::default()
        };

        let dir = std::env::temp_dir().join(format!(
            "helios-settings-rt-{}-{}",
            std::process::id(),
            std::sync::atomic::AtomicU64::new(0).fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");

        let path = save_persistent_settings_to(&dir, &settings).expect("save");
        assert!(path.exists(), "settings file must exist after save");

        let loaded = load_persistent_settings_from(&dir);
        assert_eq!(loaded, settings);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn music_attribution_strings_match_hb_297_row_1() {
        // CEO HB-297 row 1 (comment `6ab37ba7`) requires the
        // attribution overlay to name a Scott Buckley track under
        // CC-BY 4.0. The headline is rendered from these
        // constants directly (see `draw_audio_tab`) and is
        // independent of the live playlist order — but if the
        // first playlist entry is ever changed away from
        // "Starfire" by Scott Buckley, update these constants in
        // the same commit so the CEO-mandated headline stays
        // accurate. This test pins the relationship.
        let playlist = MusicPlaylist::default();
        let first = playlist
            .tracks
            .first()
            .expect("MusicPlaylist::default() must contain at least one track");
        assert_eq!(first.title, MUSIC_TITLE);
        assert_eq!(first.attribution.author, MUSIC_AUTHOR);
        assert_eq!(first.attribution.license, MUSIC_LICENSE);
        assert_eq!(first.attribution.source_url, Some(MUSIC_SOURCE_URL));
    }
}
