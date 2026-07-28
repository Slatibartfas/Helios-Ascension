//! In-game "Return to Main Menu" action (GRA-358 PR-F).
//!
//! When the player is mid-session and wants to leave the game
//! without quitting the application, the in-game options panel
//! surfaces a "🏠 Main Menu" button. Clicking it inserts a
//! [`PendingReturnToMenu`] resource; the
//! [`consume_in_game_return_to_menu_system`] reads the flag,
//! transitions [`crate::ui::launch::LaunchState`] from
//! `InGame` back to `MainMenu`, and tears down the
//! world-swap gating markers so the menu renders correctly.
//!
//! # Why a dedicated consumer
//!
//! Going `InGame` → `MainMenu` is structurally different from
//! the other `LaunchState` transitions the existing
//! [`super::transitions::consume_launch_actions_system`]
//! handles:
//!
//! - It only ever fires once per click (the resource is removed
//!   on consumption).
//! - It must clear the world-swap markers
//!   ([`super::super::persistence::swap::WorldReady`] +
//!   [`super::super::persistence::swap::RestoredWorldGate`]) so
//!   a future "New Game" kickoff re-runs the boot-init chain
//!   from scratch.
//! - It does NOT touch [`super::PendingLaunchActions`] — the
//!   return-to-main-menu button is in the in-game UI, not the
//!   launch menu shell, so the launch-action queue is the
//!   wrong transport.
//!
//! # Schedule
//!
//! `EguiPrimaryContextPass`, anchored on
//! [`super::LaunchSystemSet::Menu`] (so it runs after the
//! in-game dashboard that writes the request) and before any
//! subview render that gates on `LaunchState`. The exclusive
//! `&mut World` signature mirrors the other in-game consumers
//! (`consume_in_game_save_request_system`,
//! `consume_in_game_load_request_system`).
//!
//! # Player-visible behaviour
//!
//! - Click "🏠 Main Menu" → toast: "Returning to main menu…".
//! - World is kept in memory (the live `App`'s world); only the
//!   swap markers are reset.
//! - The menu backdrop spawns (see
//!   [`super::menu_backdrop::menu_backdrop_transition_system`])
//!   and the camera is restored to the saved pre-game framing.
//! - A subsequent "New Game" / "Continue" / "Load Save" click
//!   triggers a fresh kickoff; the boot-init chain runs because
//!   [`super::super::persistence::swap::WorldReady`] is now
//!   `None` again.

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use super::menu_backdrop::MenuBackdropActive;
use super::LaunchState;

/// One-shot resource written by the in-game options panel's
/// "🏠 Main Menu" button. `Some(true)` means "consume me this
/// frame". The resource is removed on consumption so a stale
/// `Some(true)` can never re-fire across sessions.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PendingReturnToMenu {
    pub requested: bool,
}

impl PendingReturnToMenu {
    pub fn has_any(&self) -> bool {
        self.requested
    }
}

/// Bevy system: drain [`PendingReturnToMenu`] once, transition
/// [`LaunchState`] from `InGame` to `MainMenu`, clear the
/// world-swap markers, and remove the request resource.
///
/// Order: runs in `EguiPrimaryContextPass` after the in-game
/// dashboard that writes the request. We do NOT clear the
/// in-game `ActiveMenu` selection here — the next egui pass
/// re-evaluates it against the new `LaunchState` and the
/// `main_menu_render_system`'s `current_state != MainMenu`
/// early-return takes care of the rest.
///
/// # Why we tear down the swap markers
///
/// Without removing `WorldReady`, the boot-init chain's run_if
/// would be true on the next kickoff. Without removing
/// `RestoredWorldGate`, a fresh "Continue" path would silently
/// no-op the chain (regression risk if the chain ever starts
/// doing more work for a Restore than just no-op). Tearing
/// both down keeps the New Game / Continue / Load Game paths
/// symmetric: every kickoff starts from a clean gate.
pub fn consume_in_game_return_to_menu_system(world: &mut World) {
    let requested = world
        .get_resource::<PendingReturnToMenu>()
        .map(|r| r.requested)
        .unwrap_or(false);
    if !requested {
        return;
    }
    let current = *world.resource::<LaunchState>();
    if current != LaunchState::InGame {
        // Stale request from a non-InGame state — discard.
        // (Defensive: the dashboard only writes the resource
        // from the in-game options panel, but the consumer
        // could in theory race a kickoff that already advanced
        // the state. Discarding is safer than transitioning.)
        world.remove_resource::<PendingReturnToMenu>();
        return;
    }

    info!("ReturnToMenu: in-game return request → LaunchState=MainMenu");

    // 1. Flip the launch state. The menu render system
    //    (gated on `MainMenu`) becomes active on the next
    //    frame; the menu backdrop system picks up the state
    //    change and spawns the Earth + clouds + lights.
    *world.resource_mut::<LaunchState>() = LaunchState::MainMenu;

    // 2. Clear the world-swap gating markers. The next kickoff
    //    (New Game / Continue / Load Save) will re-insert
    //    `WorldReady` and (on the Restore path) `RestoredWorldGate`.
    world.remove_resource::<crate::persistence::swap::WorldReady>();
    world.remove_resource::<crate::persistence::swap::RestoredWorldGate>();
    // GRA-358 PR-J: also clear the post-swap decoration marker
    // so a subsequent Load Save kickoff re-runs the 3D pass on
    // the freshly-swapped world.
    world.remove_resource::<crate::persistence::RestoredBodiesRendered>();

    // 3. Reset the menu-backdrop active flag so the spawn
    //    branch fires next frame. The flag is normally set on
    //    the very first frame after app boot, then sticky true
    //    until the player leaves the menu family. A return
    //    from InGame keeps `MenuBackdropActive.0 == false`
    //    (we never spawned the backdrop the first time), so
    //    the transition system will spawn it. We force `false`
    //    explicitly to make the intent obvious in tests.
    if let Some(mut active) = world.get_resource_mut::<MenuBackdropActive>() {
        active.0 = false;
    }

    // 4. Reset the boot-init gate to `Loading` so a future
    //    "New Game" kickoff re-runs the deferred-init chain
    //    from scratch. Without this, the chain stays
    //    suppressed (`Ready`) and the swap would land on a
    //    world that was never seeded with bodies / tech /
    //    fleets.
    if let Some(mut boot_state) = world.get_resource_mut::<crate::boot_init::BootState>() {
        *boot_state = crate::boot_init::BootState::Loading;
    }

    // 5. Clear any stale in-game menu state. The
    //    `ActiveMenu` resource holds the currently selected
    //    in-game submenu (Research / Fleets / etc.). On
    //    return-to-main-menu the player is leaving the in-game
    //    UI family entirely; resetting to `Survey` keeps the
    //    next in-game session's first frame deterministic.
    if let Some(mut active_menu) = world.get_resource_mut::<crate::game_state::ActiveMenu>() {
        active_menu.current = crate::game_state::GameMenu::Survey;
    }

    // 6. Consume the request resource.
    world.remove_resource::<PendingReturnToMenu>();
}

/// Register the consumer in
/// [`EguiPrimaryContextPass`]. Anchored on
/// [`super::LaunchSystemSet::Menu`] so it runs after the
/// in-game dashboard that writes the resource, and before any
/// subview render that gates on `LaunchState`. The exclusive
/// system pattern matches the other in-game consumers
/// (`consume_in_game_save_request_system`,
/// `consume_in_game_load_request_system`).
pub fn register_return_to_menu_consumer(app: &mut App) {
    app.add_systems(
        EguiPrimaryContextPass,
        consume_in_game_return_to_menu_system.in_set(super::LaunchSystemSet::Menu),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::swap::{RestoredWorldGate, WorldReady};
    use crate::ui::launch::menu_backdrop::MenuBackdropActive;
    use crate::ui::launch::LaunchState;
    use bevy::app::App;
    use bevy::MinimalPlugins;

    /// Helper: build a minimal `App` that mirrors the live
    /// app's resource set (just the ones the consumer reads /
    /// writes). We don't pull in the full plugin graph because
    /// the consumer is a pure data-mutation system and the
    /// surrounding plugins aren't relevant to its contract.
    fn fresh_app_in_game() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<LaunchState>()
            .init_resource::<crate::boot_init::BootState>()
            .init_resource::<MenuBackdropActive>()
            .init_resource::<crate::game_state::ActiveMenu>();
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::InGame;
        *app.world_mut().resource_mut::<crate::boot_init::BootState>() =
            crate::boot_init::BootState::Ready;
        app
    }

    /// GRA-358 PR-F regression: clicking "🏠 Main Menu" in
    /// the in-game options panel must advance `LaunchState` to
    /// `MainMenu` and tear down the world-swap gating markers
    /// so a subsequent New Game / Continue / Load Save
    /// kickoff re-runs the boot-init chain from scratch.
    #[test]
    fn consume_in_game_return_to_menu_advances_state_and_clears_markers() {
        let mut app = fresh_app_in_game();
        // Simulate the dashboard having inserted the swap
        // markers (the live app's `promote_pending_world` does
        // this on every kickoff).
        app.insert_resource(WorldReady);
        app.insert_resource(RestoredWorldGate);

        // Simulate the dashboard's "🏠 Main Menu" click.
        app.insert_resource(PendingReturnToMenu { requested: true });

        // Run the consumer system through the world's system
        // registry. We need to register it first so the
        // exclusive `&mut World` signature can be invoked via
        // `World::run_system` (which takes a `SystemId`, not a
        // bare function).
        let system_id = app
            .world_mut()
            .register_system(consume_in_game_return_to_menu_system);
        app.world_mut().run_system(system_id).expect("system runs cleanly");

        // State advanced to MainMenu.
        assert_eq!(
            *app.world().resource::<LaunchState>(),
            LaunchState::MainMenu,
            "return-to-menu must advance LaunchState to MainMenu"
        );

        // Swap markers cleared.
        assert!(
            app.world().get_resource::<WorldReady>().is_none(),
            "WorldReady must be removed so a future kickoff re-runs the boot-init chain"
        );
        assert!(
            app.world().get_resource::<RestoredWorldGate>().is_none(),
            "RestoredWorldGate must be removed for symmetry"
        );

        // Boot-init gate reset to Loading.
        assert_eq!(
            *app.world().resource::<crate::boot_init::BootState>(),
            crate::boot_init::BootState::Loading,
            "BootState must be reset to Loading so a New Game kickoff re-runs the chain"
        );

        // Menu backdrop active flag forced false.
        assert!(
            !app.world().resource::<MenuBackdropActive>().0,
            "MenuBackdropActive must be false so the spawn branch fires next frame"
        );

        // Request consumed.
        assert!(
            app.world().get_resource::<PendingReturnToMenu>().is_none(),
            "PendingReturnToMenu must be removed on consumption (one-shot)"
        );
    }

    /// Defensive: a stale `PendingReturnToMenu` resource left
    /// in a non-InGame state (e.g. the player pressed Esc to
    /// back out of a submenu between request-write and
    /// consumer-tick) must be discarded without mutating
    /// `LaunchState`.
    #[test]
    fn consume_in_game_return_to_menu_discards_stale_request_from_non_in_game() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<LaunchState>()
            .init_resource::<crate::boot_init::BootState>()
            .init_resource::<MenuBackdropActive>()
            .init_resource::<crate::game_state::ActiveMenu>();
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::MainMenu;
        app.insert_resource(PendingReturnToMenu { requested: true });
        app.insert_resource(WorldReady);

        let system_id = app
            .world_mut()
            .register_system(consume_in_game_return_to_menu_system);
        app.world_mut().run_system(system_id).expect("system runs cleanly");

        // State unchanged.
        assert_eq!(
            *app.world().resource::<LaunchState>(),
            LaunchState::MainMenu,
            "stale request from non-InGame must not mutate LaunchState"
        );
        // WorldReady untouched (we shouldn't have cleared it).
        assert!(
            app.world().get_resource::<WorldReady>().is_some(),
            "stale request must not tear down the swap markers"
        );
        // Request consumed (so it can't fire again next frame).
        assert!(
            app.world().get_resource::<PendingReturnToMenu>().is_none(),
            "stale request must still be consumed (one-shot)"
        );
    }

    /// Negative: no request resource → no-op.
    #[test]
    fn consume_in_game_return_to_menu_is_noop_without_request() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<LaunchState>()
            .init_resource::<crate::boot_init::BootState>()
            .init_resource::<MenuBackdropActive>()
            .init_resource::<crate::game_state::ActiveMenu>();
        *app.world_mut().resource_mut::<LaunchState>() = LaunchState::InGame;

        let system_id = app
            .world_mut()
            .register_system(consume_in_game_return_to_menu_system);
        app.world_mut().run_system(system_id).expect("system runs cleanly");

        assert_eq!(
            *app.world().resource::<LaunchState>(),
            LaunchState::InGame,
            "missing request must leave LaunchState at InGame"
        );
    }
}
