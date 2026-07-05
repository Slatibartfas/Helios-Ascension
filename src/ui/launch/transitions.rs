//! Launch-flow action consumer (GRA-329 PR-E).
//!
//! Reads [`super::PendingLaunchActions`] once per frame on the
//! `Update` schedule, advances [`super::LaunchState`], and clears
//! the queue. Distinct from PR-D's `kickoff_world_system` (which
//! fires *after* `LaunchState == InGame`): this module is the gate
//! that *reaches* `InGame`.
//!
//! Scope (per GRA-329 issue body):
//!
//! - `start_new_game` / `continue_recent` / `load_save` → set
//!   `LaunchState::InGame`.
//! - `load_save` → also stash the path in
//!   [`super::PendingLoadSave`] so the GRA-314 PR-B persistence
//!   integration can pick it up.
//! - `quit` → fire [`bevy::app::AppExit`].
//!
//! Constraints:
//!
//! - Runs in `Update`, **not** `EguiPrimaryContextPass` — pure
//!   resource mutation, no egui context required.
//! - One-frame lag is acceptable: the egui menu render writes to
//!   `PendingLaunchActions` in `EguiPrimaryContextPass`; the next
//!   frame's `Update` consumer reads + clears it; the chrome gate
//!   in `EguiPrimaryContextPass` of that next frame sees
//!   `LaunchState::InGame` and the chrome appears.

use bevy::app::AppExit;
use bevy::prelude::*;

use super::{LaunchState, NewGameParams, NewGameRequest, PendingLaunchActions};

/// One-shot resource holding the path the player selected in the
/// Load Game flow. Populated by the transition consumer; consumed
/// by the GRA-314 PR-B persistence integration (out of scope here).
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct PendingLoadSave(pub std::path::PathBuf);

/// Bevy system: consume `PendingLaunchActions` and advance
/// `LaunchState` accordingly. Clears the queue on consumption.
///
/// Runs in `Update` (which executes before `EguiPrimaryContextPass`
/// in Bevy 0.18's schedule order) so the menu button press from
/// the previous frame's egui pass is reflected next frame. The
/// chrome `run_if(in_game_chrome)` predicate evaluates against
/// the new `LaunchState::InGame` value in the same frame the
/// consumer fires. Idempotent: an empty queue is a no-op.
pub fn consume_launch_actions_system(
    mut launch_state: ResMut<LaunchState>,
    mut actions: ResMut<PendingLaunchActions>,
    mut pending_load_save: ResMut<PendingLoadSave>,
    mut app_exit: MessageWriter<AppExit>,
) {
    if !actions.has_any() {
        return;
    }

    if actions.quit
        && actions.start_new_game.is_none()
        && actions.load_save.is_none()
        && !actions.continue_recent
    {
        app_exit.write(AppExit::Success);
        actions.clear();
        return;
    }

    if let Some(path) = actions.load_save.take() {
        pending_load_save.0 = path;
    }

    actions.clear();
    *launch_state = LaunchState::InGame;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::{LaunchState, NewGameRequest, PendingLaunchActions};
    use bevy::MinimalPlugins;

    fn fresh_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<LaunchState>();
        app.init_resource::<PendingLaunchActions>();
        app.init_resource::<PendingLoadSave>();
        app.add_message::<AppExit>();
        app.add_systems(Update, consume_launch_actions_system);
        app
    }

    #[test]
    fn empty_queue_is_noop() {
        let mut app = fresh_app();
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::MainMenu;
        app.update();
        assert_eq!(
            *app.world().resource::<LaunchState>(),
            LaunchState::MainMenu
        );
        assert!(!app.world().resource::<PendingLaunchActions>().has_any());
    }

    #[test]
    fn start_new_game_advances_to_in_game() {
        let mut app = fresh_app();
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::MainMenu;
        app.world_mut()
            .resource_mut::<PendingLaunchActions>()
            .start_new_game = Some(NewGameRequest {
            params: NewGameParams::default(),
            seed: 0,
            preset: "standard".into(),
        });

        app.update();

        assert!(app.world().resource::<LaunchState>().is_in_game());
        assert!(!app.world().resource::<PendingLaunchActions>().has_any());
    }

    #[test]
    fn quit_only_fires_app_exit() {
        let mut app = fresh_app();
        app.world_mut().resource_mut::<PendingLaunchActions>().quit = true;

        app.update();

        let messages = app.world().resource::<Messages<AppExit>>();
        assert!(!messages.is_empty());
        assert_eq!(*app.world().resource::<LaunchState>(), LaunchState::Splash);
        assert!(!app.world().resource::<PendingLaunchActions>().has_any());
    }
}
