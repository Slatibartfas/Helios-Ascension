//! Launch flow plugin — splash screen, main menu, and the
//! settings/save persistence that ties them together.
//!
//! Design parent: GRA-309 (`docs/design/gra-309-splash-and-main-menu.md`,
//! comment id `1abbb963-e10a-460f-a24b-f0ae41cf5137`).
//!
//! PR-A (GRA-311, this PR) is the **skeleton**: types, plugin, RON
//! manifest loader, [`PersistentSettings`] + userdata helpers, and
//! the read-only [`SaveIndex`] stub. No rendering systems land here —
//! those follow in:
//!
//! - **PR-B / GRA-312** — splash rendering + auto-dismiss + transition
//!   to `LaunchState::MainMenu`.
//! - **PR-C / GRA-313** — main menu shell + 5 buttons + key bindings.
//! - **PR-D / GRA-314** — subviews (New / Load / Settings).
//!
//! Per [[feedback-egui-render-tests]], no egui render tests are added
//! here — the PR-A test plan is type/IO round-trips only.

pub mod manifest;
pub mod save_index;
pub mod splash;
pub mod userdata;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use std::path::PathBuf;

pub use manifest::{load_launch_ui_manifest, LaunchUiManifest};
pub use save_index::{SaveHeader, SaveIndex, SaveSummary, SAVES_SUBDIR};
pub use splash::{ui_splash_system, SplashImage, SplashTimer};
pub use userdata::{
    load_persistent_settings_from, resolve_userdata_dir, save_persistent_settings_to,
    PersistentSettings, SETTINGS_FILE_NAME,
};

/// Top-level launch-flow state machine (GRA-309 §3.3).
///
/// Default is `Splash` — the app boots into the splash screen and a
/// PR-B system advances to `MainMenu`. `InGame` represents "past the
/// menu, simulation running" and is the state the in-game UI
/// (`MainPanels`, top menu bar) cares about.
///
/// PR-A inserts this resource at Startup; PR-B writes the transition
/// systems that move between variants.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchState {
    #[default]
    Splash,
    MainMenu,
    NewGame,
    LoadGame,
    Settings,
    InGame,
}

impl LaunchState {
    /// True when the in-game UI (`MainPanels`, top menu bar) should be
    /// drawing. PR-A does not consume this — it is here so PR-B/C can
    /// gate the splash/menu render without inventing a parallel enum.
    pub fn is_in_game(&self) -> bool {
        matches!(self, LaunchState::InGame)
    }
}

/// One-shot actions the menu can request (GRA-309 §3.3).
///
/// UI systems write here; a transition system consumes + clears. The
/// shape is intentionally narrow in PR-A — PR-C (main menu shell) and
/// PR-D (subviews) will be the only writers, so the PR-A skeleton
/// can stay minimal.
#[derive(Resource, Debug, Default, PartialEq)]
pub struct PendingLaunchActions {
    pub start_new_game: Option<NewGameRequest>,
    pub continue_recent: bool,
    pub load_save: Option<PathBuf>,
    pub quit: bool,
}

impl PendingLaunchActions {
    /// True when at least one action is queued. PR-B/C transition
    /// systems poll this to decide whether to advance `LaunchState`.
    pub fn has_any(&self) -> bool {
        self.start_new_game.is_some()
            || self.continue_recent
            || self.load_save.is_some()
            || self.quit
    }

    /// Clear all queued actions. Called by the transition system
    /// after consumption.
    pub fn clear(&mut self) {
        self.start_new_game = None;
        self.continue_recent = false;
        self.load_save = None;
        self.quit = false;
    }
}

/// Request payload for "New Game" — populated by PR-D's new-game
/// subview from a [`crate::ui::launch::userdata::PersistentSettings`]
/// snapshot and the selected preset id (LGD-owned
/// `assets/data/difficulty_presets.ron`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGameRequest {
    pub seed: u64,
    pub preset: String,
}

/// System sets for the launch flow (GRA-309 §3.3).
///
/// `Splash` runs while `LaunchState == Splash`; `Menu` runs while
/// `LaunchState` is `MainMenu`, `NewGame`, `LoadGame`, or `Settings`.
/// PR-A registers the sets but ships no systems inside them — the
/// render systems land in PR-B / PR-C / PR-D.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchSystemSet {
    /// Splash rendering + auto-dismiss logic. PR-B.
    Splash,
    /// Main menu + subview rendering + key bindings. PR-C / PR-D.
    Menu,
}

/// Plugin that wires the launch-flow skeleton into Bevy.
///
/// PR-A registered the resources, the manifest loader, the
/// `PersistentSettings` reader, and the `SaveIndex` scanner. PR-B
/// (GRA-312) adds the splash render system, the `SplashTimer` /
/// `SplashImage` resources, and reconfigures `LaunchSystemSet::Splash`
/// for `EguiPrimaryContextPass` (per `helios-architecture` — egui
/// systems must run in the primary context pass, not `Update`).
pub struct LaunchPlugin;

impl Plugin for LaunchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LaunchState>()
            .init_resource::<PendingLaunchActions>()
            .init_resource::<PersistentSettings>()
            .init_resource::<SplashTimer>()
            .init_resource::<SplashImage>()
            // RON manifest from assets/data/launch_ui.ron (LGD-owned
            // content per GRA-310).
            .add_systems(Startup, load_launch_ui_manifest)
            // Save index scanner — reads the saves directory at
            // Startup so the menu has its list before the first
            // frame draws.
            .add_systems(Startup, load_save_index_system)
            // PR-B: reserve the splash set in `Update` as well so
            // other systems (e.g. PR-C's input polling) can join
            // without re-importing the schedule type. The splash
            // render itself lives in `EguiPrimaryContextPass`.
            .configure_sets(
                Update,
                (LaunchSystemSet::Splash, LaunchSystemSet::Menu).chain(),
            )
            .configure_sets(
                EguiPrimaryContextPass,
                LaunchSystemSet::Splash,
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_splash_system.in_set(LaunchSystemSet::Splash),
            );
    }
}

/// Startup system: read settings from disk into the
/// [`PersistentSettings`] resource, then run the save-index scanner.
///
/// Two distinct calls because settings load touches the resource
/// directly (`init_resource` already gave us a default), while the
/// save-index scanner inserts a fresh resource via
/// [`crate::ui::launch::save_index::SaveIndex::scan`].
fn load_save_index_system(world: &mut World) {
    let dir = resolve_userdata_dir();
    let settings = load_persistent_settings_from(&dir);
    world.insert_resource(settings);

    let saves_dir = dir.join(SAVES_SUBDIR);
    let index = SaveIndex::scan(&saves_dir);
    info!(
        "SaveIndex: {} valid + {} broken save(s) in {}",
        index.valid_count(),
        index.broken_count(),
        saves_dir.display()
    );
    world.insert_resource(index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_state_default_is_splash() {
        assert_eq!(LaunchState::default(), LaunchState::Splash);
    }

    #[test]
    fn launch_state_is_in_game_only_for_in_game_variant() {
        assert!(!LaunchState::Splash.is_in_game());
        assert!(!LaunchState::MainMenu.is_in_game());
        assert!(!LaunchState::NewGame.is_in_game());
        assert!(!LaunchState::LoadGame.is_in_game());
        assert!(!LaunchState::Settings.is_in_game());
        assert!(LaunchState::InGame.is_in_game());
    }

    #[test]
    fn pending_launch_actions_default_is_empty() {
        let actions = PendingLaunchActions::default();
        assert!(!actions.has_any());
    }

    #[test]
    fn pending_launch_actions_clear_resets_every_field() {
        let mut actions = PendingLaunchActions {
            start_new_game: Some(NewGameRequest {
                seed: 42,
                preset: "standard".to_string(),
            }),
            continue_recent: true,
            load_save: Some(PathBuf::from("/tmp/x.ron")),
            quit: true,
        };
        assert!(actions.has_any());
        actions.clear();
        assert!(!actions.has_any());
        assert_eq!(actions, PendingLaunchActions::default());
    }

    #[test]
    fn new_game_request_is_comparable() {
        let a = NewGameRequest {
            seed: 1,
            preset: "casual".to_string(),
        };
        let b = NewGameRequest {
            seed: 1,
            preset: "casual".to_string(),
        };
        let c = NewGameRequest {
            seed: 2,
            preset: "casual".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
