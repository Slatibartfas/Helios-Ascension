//! Launch-flow action consumer (GRA-329 PR-E).
//!
//! Reads [`super::PendingLaunchActions`] once per frame on the
//! `Update` schedule and advances [`super::LaunchState`]. Distinct
//! from PR-D's `kickoff_world_system` (which fires *after*
//! `LaunchState == InGame`): this module is the gate that *reaches*
//! `InGame`.
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
//! # GRA-358 PR-B / PR-C: the consumer does NOT clear actions
//!
//! Pre-PR-B this module called `actions.clear()` at the end of
//! the `Update` tick. The kickoff ran in `EguiPrimaryContextPass`
//! of the *next* frame, by which time the actions queue was
//! empty — `actions.has_any() == false` — and the kickoff had
//! nothing to do. The New Game path worked anyway because the
//! new-game subview's "Begin" click handler writes BOTH
//! `actions.start_new_game` AND `*launch_state = InGame` in the
//! same egui pass, so the kickoff (which runs in the same pass,
//! after the subview's `LaunchSystemSet::Menu`) saw the action.
//! The Continue / Load Game paths were silently broken: the main
//! menu's "Continue" click only sets `actions.continue_recent`
//! (no state transition), and the Load Game subview writes the
//! path + state to InGame from a different render system.
//!
//! PR-C changes the contract: the consumer only advances
//! `LaunchState` and stashes the load path. It does NOT clear
//! `PendingLaunchActions` — the kickoff clears the fields it
//! consumes, so the action is visible to the kickoff in the
//! next `EguiPrimaryContextPass` (1 frame after the consumer
//! fires). The Quit path is unchanged (it writes `AppExit` and
//! clears the action atomically — there's no second consumer).
//!
//! Constraints:
//!
//! - Runs in `Update`, **not** `EguiPrimaryContextPass` — pure
//!   resource mutation, no egui context required.

use bevy::app::AppExit;
use bevy::prelude::*;

use super::{LaunchState, PendingLaunchActions};

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

    // Stash the load path so the kickoff (PR-D) can pick it up
    // in the next `EguiPrimaryContextPass`. We use `take()` here
    // because the path is a one-shot — a second kickoff cycle
    // should not re-restore the same save.
    if let Some(path) = actions.load_save.take() {
        pending_load_save.0 = path;
    }

    // Advance state. Do NOT clear `start_new_game` or
    // `continue_recent` — the kickoff (which runs in the next
    // `EguiPrimaryContextPass` tick) reads them and clears
    // them itself. Clearing them here would race the kickoff
    // and produce a permanently empty world on Continue /
    // Load Game (the "player clicks New Game, lands in empty
    // world" bug from the 2a6b969 era). See the module doc
    // for the full contract.
    *launch_state = LaunchState::InGame;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::{LaunchState, NewGameParams, NewGameRequest, PendingLaunchActions};
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
        // GRA-358 PR-C contract: the consumer does NOT clear
        // `start_new_game` — the kickoff (in the next
        // `EguiPrimaryContextPass`) consumes it and clears the
        // field itself. Asserting `has_any() == false` here
        // would lock in the broken pre-PR-B behaviour.
        assert!(
            app.world()
                .resource::<PendingLaunchActions>()
                .start_new_game
                .is_some(),
            "consumer must leave the action visible to the next-frame kickoff"
        );
    }

    #[test]
    fn quit_only_fires_app_exit() {
        let mut app = fresh_app();
        app.world_mut().resource_mut::<PendingLaunchActions>().quit = true;

        app.update();

        let messages = app.world().resource::<Messages<AppExit>>();
        assert!(!messages.is_empty());
        // The state is unchanged by a quit-only action. Default is
        // `MainMenu` (the splash moved to a pre-main Bevy app,
        // see `splash_standalone`).
        assert_eq!(
            *app.world().resource::<LaunchState>(),
            LaunchState::MainMenu
        );
        assert!(!app.world().resource::<PendingLaunchActions>().has_any());
    }

    /// GRA-358 PR-C: the consumer advances `LaunchState` to
    /// `InGame` but does NOT clear `start_new_game` (the kickoff
    /// in the next `EguiPrimaryContextPass` consumes it). If the
    /// consumer did clear, the kickoff would observe an empty
    /// action queue and the world-swap would never fire. This
    /// test pins the new contract: the action survives the
    /// consumer, the state is in-game, and a follow-up kickoff
    /// can pick it up.
    #[test]
    fn consumer_advances_state_without_clearing_action() {
        let mut app = fresh_app();
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::MainMenu;
        app.world_mut()
            .resource_mut::<PendingLaunchActions>()
            .start_new_game = Some(NewGameRequest {
            params: NewGameParams::default(),
            seed: 42,
            preset: "standard".into(),
        });

        app.update();

        // State advanced.
        assert_eq!(
            *app.world().resource::<LaunchState>(),
            LaunchState::InGame
        );
        // Action still visible to the next-frame kickoff.
        let actions = app.world().resource::<PendingLaunchActions>();
        assert!(
            actions.start_new_game.is_some(),
            "consumer must NOT clear start_new_game; the kickoff clears it on consumption"
        );
        assert_eq!(
            actions.start_new_game.as_ref().unwrap().seed,
            42,
            "consumer must NOT mutate the action payload"
        );
    }
}
