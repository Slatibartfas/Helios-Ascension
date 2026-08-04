//! Launch flow plugin — main menu, settings/save persistence, and the
//! subview handoff.
//!
//! Design parent: GRA-309 (`docs/design/gra-309-splash-and-main-menu.md`,
//! comment id `1abbb963-e10a-460f-a24b-f0ae41cf5137`).
//!
//! History:
//!
//! - PR-A (GRA-311) shipped the **skeleton**: types, plugin, RON
//!   manifest loader, [`PersistentSettings`] + userdata helpers, and
//!   the read-only [`SaveIndex`] stub.
//! - PR-B (GRA-316) added the splash render + `EguiPrimaryContextPass`
//!   binding on top.
//! - PR-C (GRA-317) added the main menu shell + `LaunchSystemSet::Menu`
//!   `EguiPrimaryContextPass` registration.
//! - PR-D (GRA-318) wired the three subviews (New / Load / Settings)
//!   and the `kickoff_world_system` that consumes
//!   `PendingLaunchActions` once `LaunchState` reaches `InGame`.
//! - PR-E (GRA-329) layered the action-consumer system that actually
//!   advances `LaunchState` to `InGame`, fires `AppExit` for `quit`,
//!   and publishes the load-save path into [`PendingLoadSave`].
//! - GRA-3xx PR-A (this revision) moved the splash into its own
//!   OS-level Window entity so the player never sees the 1920×1080
//!   game window during the splash. The splash lives in
//!   [`crate::ui::launch::splash`] and is registered via
//!   [`SplashPlugin`] from `src/main.rs`. The launch state machine
//!   no longer has a `Splash` variant — it boots straight into
//!   `MainMenu` because the splash window is dismissed before the
//!   main menu is visible (the main window has `visible: false`
//!   during the splash).
//!
//! Per [[feedback-egui-render-tests]], no egui render tests are
//! added here.

pub mod manifest;
pub mod menu;
pub mod menu_backdrop;
pub mod return_to_menu;
pub mod save_index;
pub mod splash;
pub mod transitions;
pub mod userdata;

pub mod subview_kickoff;
pub mod subview_load_game;
pub mod subview_manifests;
pub mod subview_new_game;
pub mod subview_save_game;
pub mod subview_settings;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use std::path::PathBuf;

pub use manifest::{load_launch_ui_manifest, LaunchUiManifest};
pub use menu::{main_menu_render_system, render_glass_button};
pub use menu_backdrop::{
    MenuBackdropActive, MenuBackdropKind, MenuBackdropMarker, MenuBackdropPlugin,
};
pub use save_index::{SaveHeader, SaveIndex, SaveIndexState, SaveSummary, SAVES_SUBDIR};
// Splash types re-exported so callers can `use crate::ui::launch::SplashPlugin`
// without reaching into `splash::*` directly.
pub use return_to_menu::{
    consume_in_game_return_to_menu_system, register_return_to_menu_consumer, PendingReturnToMenu,
};
pub use splash::{SplashContextPass, SplashImageData, SplashPlugin, SplashTimer};
pub use subview_load_game::PendingInGameLoadRequest;
pub use subview_manifests::{load_difficulty_presets_manifest, load_seed_copy_manifest};
pub use subview_save_game::{
    consume_in_game_save_request_system, consume_save_actions_system, register_save_panel_subview,
    ui_save_panel_subview, PendingInGameSaveRequest, PendingSaveActions, PendingSavePanelReturn,
};
pub use transitions::{consume_launch_actions_system, PendingLoadSave};
pub use userdata::{
    load_persistent_settings_from, resolve_userdata_dir, save_persistent_settings_to,
    PersistentSettings, SETTINGS_FILE_NAME,
};

pub use crate::persistence::params::{
    load_new_game_params_defaults, NewGameParams, NewGameParamsDefaults,
};

/// Top-level launch-flow state machine (GRA-309 §3.3).
///
/// `LaunchState::Splash` was removed when the splash moved to a
/// pre-main Bevy app (see `splash_standalone`); the game app boots
/// straight into `MainMenu`. The `Splash` variant was a historical
/// artifact of the in-window splash and is no longer reachable.
///
/// `InGame` represents "past the menu, simulation running" and is
/// the state the in-game UI (`MainPanels`, top menu bar) cares
/// about.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchState {
    #[default]
    MainMenu,
    NewGame,
    LoadGame,
    Settings,
    SaveGame,
    InGame,
}

impl LaunchState {
    /// True when the in-game UI (`MainPanels`, top menu bar) should
    /// be drawing. PR-A does not consume this — it is here so PR-C
    /// can gate the menu render without inventing a parallel enum.
    pub fn is_in_game(&self) -> bool {
        matches!(self, LaunchState::InGame)
    }
}

/// One-shot actions the menu can request (GRA-309 §3.3).
///
/// UI systems write here; a transition system consumes + clears.
/// PR-D (subviews) is the only writer.
#[derive(Resource, Debug, Default, PartialEq)]
pub struct PendingLaunchActions {
    pub start_new_game: Option<NewGameRequest>,
    pub continue_recent: bool,
    pub load_save: Option<PathBuf>,
    pub quit: bool,
}

impl PendingLaunchActions {
    /// True when at least one action is queued. Transition systems
    /// poll this to decide whether to advance `LaunchState`.
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

/// Request payload for "New Game" — populated by the new-game
/// subview from a [`crate::ui::launch::userdata::PersistentSettings`]
/// snapshot and the selected preset id (LGD-owned
/// `assets/data/difficulty_presets.ron`).
///
/// GRA-358 PR-A added `params: NewGameParams` (star count, AI faction
/// count, artifacts toggle, starting tech tier, initial game speed)
/// alongside the existing `seed` + `preset`. The kickoff world-spawn
/// path (a follow-up PR) reads `params` to drive procedural
/// generation; the seed remains the canonical RNG seed and the
/// preset remains the difficulty identity.
#[derive(Debug, Clone, PartialEq)]
pub struct NewGameRequest {
    pub params: NewGameParams,
    pub seed: u64,
    pub preset: String,
}

/// System sets for the launch flow (GRA-309 §3.3).
///
/// `Menu` runs while `LaunchState` is `MainMenu`, `NewGame`,
/// `LoadGame`, or `Settings`. The `Splash` set was removed when the
/// splash moved to a pre-main Bevy app (`splash_standalone`); see
/// the module doc for context.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchSystemSet {
    /// Main menu + subview rendering + key bindings + kickoff
    /// transition. PR-C / PR-D.
    Menu,
}

/// Plugin that wires the launch-flow skeleton into Bevy.
///
/// GRA-3xx PR-A removed the splash render + `EguiPrimaryContextPass`
/// binding for `LaunchSystemSet::Splash`; the splash now runs in a
/// pre-main Bevy app (`splash_standalone::build_splash_app`). The
/// `LaunchState::Splash` variant was removed at the same time; the
/// game app boots into `LaunchState::MainMenu`.
pub struct LaunchPlugin;

impl Plugin for LaunchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LaunchState>()
            .init_resource::<PendingLaunchActions>()
            .init_resource::<PendingLoadSave>()
            .init_resource::<PersistentSettings>()
            // PR-E (GRA-329) writes `AppExit` via `MessageWriter`.
            // Bevy 0.18 split Events → Messages; `AppExit` is a
            // Message, so register it explicitly to avoid the
            // "Message not initialized" panic documented in
            // [[feedback-bevy-018-add-message-when-adding-writer]].
            .add_message::<bevy::app::AppExit>()
            // RON manifests from assets/data/launch_*.ron (LGD-owned
            // content per GRA-310). Order matters: launch_ui must
            // load before the menu render reads it; the preset +
            // seed_copy manifests must load before the subview
            // render systems read them. Bevy's `Startup` schedule
            // respects insertion order.
            .add_systems(Startup, load_launch_ui_manifest)
            .add_systems(Startup, load_difficulty_presets_manifest)
            .add_systems(Startup, load_seed_copy_manifest)
            // GRA-358 PR-A: new_game_params defaults (LGD-authored
            // RON; see assets/data/new_game_params.ron). The loader
            // inserts the `NewGameParamsDefaults` resource before
            // the New Game subview first renders so the slider
            // defaults and the soft star-count ceiling are
            // available. The loader is registered last because it
            // does not gate any of the earlier manifests' content.
            .add_systems(Startup, load_new_game_params_defaults)
            // Save index scanner — reads the saves directory at
            // Startup so the menu has its list before the first
            // frame draws.
            .add_systems(Startup, load_save_index_system)
            // PR-C registered the `Menu` set on `EguiPrimaryContextPass`
            // so the egui `main_menu_render_system` can write to the
            // active egui context (the in-game tooltip hard-rule,
            // CLAUDE.md "Egui Scheduling"). The Splash set is gone —
            // see `splash_standalone` for where the splash lives
            // now.
            .configure_sets(Update, LaunchSystemSet::Menu)
            .configure_sets(EguiPrimaryContextPass, LaunchSystemSet::Menu)
            .add_systems(
                EguiPrimaryContextPass,
                main_menu_render_system.in_set(LaunchSystemSet::Menu),
            );

        // PR-D subviews: each owns its render system + the resource
        // it needs and registers them on `EguiPrimaryContextPass`
        // inside `LaunchSystemSet::Menu`. The render systems gate
        // themselves on `LaunchState` so the `NewGame` subview
        // no-ops when the player is on `LoadGame`.
        subview_new_game::register_new_game_subview(app);
        subview_load_game::register_load_game_subview(app);
        subview_settings::register_settings_subview(app);
        subview_save_game::register_save_panel_subview(app);
        subview_kickoff::register_kickoff_system(app);
        // GRA-358 PR-F: in-game "🏠 Main Menu" button. The
        // consumer reads the one-shot `PendingReturnToMenu`
        // resource written by the in-game options panel and
        // transitions `LaunchState` from `InGame` back to
        // `MainMenu`, clearing the world-swap markers so a
        // subsequent kickoff re-runs the boot-init chain.
        return_to_menu::register_return_to_menu_consumer(app);

        // GRA-XYZ: menu backdrop — spawns the rotating-Earth close-up
        // and ambient/sun lighting for the menu session, despawns on
        // the menu→InGame transition. See `menu_backdrop.rs`.
        menu_backdrop::register_menu_backdrop_plugin(app);

        // Push `PersistentSettings::window_mode` to `Window::mode`
        // on the primary window. Registered in `Update` so the
        // settings subview's mutations (which happen in
        // `EguiPrimaryContextPass`) take effect on the same frame.
        // See `src/plugins/window_mode_bridge.rs` for the system.
        crate::plugins::window_mode_bridge::register_window_mode_bridge(app);

        // DX12-safe path for the window minimize crash. Drains
        // `WindowResized` in `Last` (after `bevy_winit`'s
        // `changed_windows`) and substitutes a sane non-zero
        // resolution for the primary window when the OS reports
        // 0×0. Registered here, not in `main.rs`, so the order
        // matches `LaunchPlugin`'s ownership of the settings
        // machinery.
        crate::plugins::minimize_guard::register_minimize_guard(app);

        // PR-E (GRA-329): action consumer runs in `Update` (not in
        // `EguiPrimaryContextPass`) — pure resource mutation, no
        // egui context required. Pairs with PR-D's
        // `kickoff_world_system`: this consumer advances
        // `LaunchState` to `InGame`; PR-D's kickoff runs *after*
        // `InGame` and decides what world to spin up.
        app.add_systems(Update, consume_launch_actions_system);
    }
}

/// Startup system: read settings from disk into the
/// [`PersistentSettings`] resource, then run the save-index
/// scanner.
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
    fn launch_state_default_is_main_menu() {
        // The splash runs in its own Bevy app before this one
        // (see splash_standalone), so the game app boots
        // straight into the main menu rather than cycling
        // through a Splash state.
        assert_eq!(LaunchState::default(), LaunchState::MainMenu);
    }

    #[test]
    fn launch_state_is_in_game_only_for_in_game_variant() {
        assert!(!LaunchState::MainMenu.is_in_game());
        assert!(!LaunchState::NewGame.is_in_game());
        assert!(!LaunchState::LoadGame.is_in_game());
        assert!(!LaunchState::Settings.is_in_game());
        assert!(!LaunchState::SaveGame.is_in_game());
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
                params: NewGameParams::default(),
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
            params: NewGameParams::default(),
            seed: 1,
            preset: "casual".to_string(),
        };
        let b = NewGameRequest {
            params: NewGameParams::default(),
            seed: 1,
            preset: "casual".to_string(),
        };
        let c = NewGameRequest {
            params: NewGameParams::default(),
            seed: 2,
            preset: "casual".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
