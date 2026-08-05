//! World-kickoff transition (GRA-318 PR-D + GRA-358 PR-C).
//!
//! Owns the `LaunchState::InGame` transition: once the player has
//! confirmed an action via one of the PR-D subviews (Begin /
//! load-save click / Settings change), the action queue holds a
//! request, and this module turns the request into a world-state
//! decision.
//!
//! Scope (per GRA-318 / GRA-358):
//!
//! - On `LaunchState::InGame` and `PendingLaunchActions::has_any()`,
//!   consume the action and decide what world to spin up:
//!   - `start_new_game` → invoke
//!     [`crate::persistence::play_new_game`] to build the fresh
//!     world and emit a [`crate::persistence::NewGameCommitted`]
//!     message.
//!   - `load_save` → invoke
//!     [`crate::persistence::restore_save`]. Failures surface as a
//!     toast via the GRA-137 notification bridge.
//!   - `continue_recent` → take the first valid entry from
//!     [`SaveIndex`] and route it through `load_save`.
//! - The [`resolve_kickoff`] helper is a pure function on resources
//!   so tests can drive it without a Bevy schedule / egui context.
//!   The [`kickoff_world_system`] is the thin Bevy adapter that
//!   calls into the [`crate::persistence::game_setup`] constructors.
//!
//! Constraints:
//!
//! - Per `feedback-egui-render-tests`, no egui render tests; the
//!   state-machine contract is verified via the `tests` module below.
//! - Quitting (`PendingLaunchActions::quit`) is intentionally NOT
//!   handled here — that route is owned by the menu shell (`PR-C`)
//!   and the app-exit surface, not by the kickoff.

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use super::save_index::{SaveIndex, SaveSummary};
use super::{LaunchState, LaunchSystemSet, NewGameRequest, PendingLaunchActions};
use crate::persistence::{play_new_game, restore_save};

/// Outcome of one kickoff decision. The Bevy system logs at the
/// `info!` level and clears the action queue on success; the
/// decision helper returns this enum so tests can assert.
#[derive(Debug, Clone, PartialEq)]
pub enum KickoffOutcome {
    /// A fresh world was queued for the New Game path. The seed is
    /// `Some(value)` when the player supplied one (random presets
    /// auto-roll in the live game; PR-D keeps `seed = 0` as the
    /// "auto" sentinel so the test contract is exact and a future
    /// PR can replace it with the auto-roll).
    StartNewGame { request: NewGameRequest },
    /// `load_save` path: the requested save path is the world to
    /// restore.
    LoadSave {
        path: std::path::PathBuf,
        source: KickoffLoadSource,
    },
    /// Nothing matched: this can happen when the player clicks Back
    /// from a subview (state was set but `actions` was cleared by
    /// the subview code). The system is a no-op.
    NoAction,
}

/// Where a `LoadSave` decision came from. The Continue button
/// routes through the SaveIndex path; explicit clicks through the
/// `LoadGame` subview come from the click site. The enum lets tests
/// distinguish "we resumed a save" from "we picked one".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KickoffLoadSource {
    /// Continue button picked the most-recent valid save.
    ContinueRecent,
    /// Player clicked a row in the Load Game subview.
    SubviewClick,
}

/// Pure decision helper. Reads the action queue + SaveIndex, decides
/// what to kick off, and records the decision in the returned
/// [`KickoffOutcome`]. The caller (system or test) decides whether
/// to actually take the action.
///
/// Does NOT clear the action queue or mutate `LaunchState` — those
/// are system-layer concerns (clearing the queue inside a const
/// helper makes tests mutate resources they didn't expect to
/// change). The system wrapper below applies both side-effects.
pub fn resolve_kickoff(
    launch_state: LaunchState,
    actions: &PendingLaunchActions,
    save_index: &SaveIndex,
) -> KickoffOutcome {
    if launch_state != LaunchState::InGame {
        return KickoffOutcome::NoAction;
    }
    if !actions.has_any() {
        return KickoffOutcome::NoAction;
    }
    // Quit is owned by the app-exit surface, not kickoff. Leave
    // this action in the queue and report no kickoff.
    if actions.quit
        && actions.start_new_game.is_none()
        && actions.load_save.is_none()
        && !actions.continue_recent
    {
        return KickoffOutcome::NoAction;
    }

    // Continue path — explicitly click wins over Continue if both
    // are queued; explicit `load_save` is more specific. The
    // Continue button is the only writer of `continue_recent` (PR-C).
    if actions.continue_recent && actions.load_save.is_none() && actions.start_new_game.is_none() {
        if let Some(path) = most_recent_valid_save(save_index) {
            return KickoffOutcome::LoadSave {
                path,
                source: KickoffLoadSource::ContinueRecent,
            };
        }
        // No valid save → fall through to whatever else is queued.
        // (Continue is disabled when SaveIndex is empty per PR-C,
        // so this branch is mostly defensive.)
    }

    if let Some(path) = actions.load_save.as_ref() {
        return KickoffOutcome::LoadSave {
            path: path.clone(),
            source: KickoffLoadSource::SubviewClick,
        };
    }

    if let Some(request) = actions.start_new_game.as_ref() {
        return KickoffOutcome::StartNewGame {
            request: request.clone(),
        };
    }

    KickoffOutcome::NoAction
}

/// Pick the most-recent valid save from `SaveIndex`. The index is
/// sorted by file name when scanned (matches the OS file listing
/// order), so the first valid entry after sorting is the most
/// recently modified. We don't filter by `saved_at` here because
/// `saved_at` is optional on the header — file-name order is the
/// canonical key for this PR.
fn most_recent_valid_save(index: &SaveIndex) -> Option<std::path::PathBuf> {
    index.entries.iter().find_map(|entry| match entry {
        SaveSummary::Valid { path, .. } => Some(path.clone()),
        SaveSummary::Broken { .. } => None,
    })
}

/// Bevy system: consume `PendingLaunchActions` once `LaunchState`
/// reaches `InGame`, invoke the [`crate::persistence`] constructors,
/// and clear the queue.
///
/// Schedule: [`EguiPrimaryContextPass`] — placed there because the
/// queue and `LaunchState` are mutated from egui subviews in the
/// same pass and the kickoff must observe the latest values within
/// the same frame. The system is also a no-op for any other state
/// so it does not conflict with splash or in-game UI ordering.
///
/// GRA-358 PR-C: this system is now an *exclusive* system (`&mut World`)
/// because [`play_new_game`] / [`restore_save`] mutate multiple
/// resources via `&mut World`, and Bevy 0.18 forbids holding
/// `&mut World` alongside `Res` / `ResMut` in the same system
/// (they conflict at the SystemParam-borrow level). We pull each
/// resource via `world.resource()` / `world.resource_mut()` instead.
///
/// On `LoadSave`, failure paths emit a `persistence.restore_failed`
/// toast via the GRA-137 notification bridges.
pub fn kickoff_world_system(world: &mut World) {
    let launch_state = *world.resource::<LaunchState>();
    let actions = world.resource::<PendingLaunchActions>();
    let save_index = world.resource::<SaveIndex>();
    let outcome = resolve_kickoff(launch_state, actions, save_index);

    match outcome {
        KickoffOutcome::StartNewGame { request } => {
            info!(
                "kickoff: StartNewGame (preset={preset}, seed={seed})",
                preset = request.preset,
                seed = request.seed
            );
            match play_new_game(world, request) {
                Ok(seed) => info!("kickoff: play_new_game committed (seed={seed})"),
                Err(e) => warn!("kickoff: play_new_game failed: {e}"),
            }
            // Clear sibling actions so a stale Continue / Load
            // queue doesn't double-fire on the next decision cycle.
            let mut actions = world.resource_mut::<PendingLaunchActions>();
            actions.continue_recent = false;
            actions.load_save = None;
            actions.start_new_game = None;
        }
        KickoffOutcome::LoadSave { path, source } => {
            info!(
                "kickoff: LoadSave (source={source:?}, path={p})",
                source = source,
                p = path.display()
            );
            // v0.5.2 (2026-08-05): hand the boot pre-parse cache to
            // the restore factory so `regenerate_bodies_minimal` +
            // `populate_restored_bodies_3d` don't each re-read +
            // re-decode `solar_system.ron` synchronously (the old
            // restore path parsed it twice while New Game got it
            // free via the async pre-parse). Falls back to a sync
            // load when the cache is `None` (player clicked before
            // the pre-parse finished).
            let cached_solar = world
                .get_resource::<crate::boot_init::BootPreParseState>()
                .and_then(|p| p.solar_data.clone());
            let factory = move || {
                crate::persistence::game_setup::build_minimal_world_for_restore_cached(
                    cached_solar,
                )
            };
            match restore_save(world, &path, factory) {
                Ok(()) => info!("kickoff: restore_save committed"),
                Err(e) => {
                    // `restore_save` already wrote a
                    // `persistence.restore_failed` toast; the log
                    // here is the operator-level trail.
                    warn!("kickoff: restore_save failed: {e}");
                }
            }
            let mut actions = world.resource_mut::<PendingLaunchActions>();
            actions.continue_recent = false;
            actions.start_new_game = None;
            actions.load_save = None;
        }
        KickoffOutcome::NoAction => {
            // No-op. We do not clear `quit` because the app-exit
            // surface (separate from this module) handles it.
        }
    }
}

/// Register the kickoff system on the `EguiPrimaryContextPass`
/// schedule, AFTER the subview render systems (all of which are in
/// `LaunchSystemSet::Menu`).
///
/// GRA-358 PR-D + GRA-358 PR-C: the system takes `&mut World` (it
/// calls into the [`crate::persistence`] constructors which mutate
/// many resources). Bevy 0.18 exclusive systems can't chain
/// `.in_set(...)`, so the order is enforced via
/// `.after(LaunchSystemSet::Menu)` (the set all subview render
/// systems belong to). The exclusive-system `.after(...)` pattern
/// is already used by `tick_autosave_timer` in
/// [`crate::persistence::plugin`].
///
/// The kickoff MUST run in the same `EguiPrimaryContextPass` pass
/// as the subview that writes `actions.start_new_game` /
/// `*launch_state = InGame`. The subview's `ResMut<LaunchState>`
/// write is committed at the end of the egui subview's call frame
/// (Bevy 0.18's `ResMut` is immediate, not deferred); a sibling
/// system in the same schedule tick observes the new value. If
/// the kickoff were moved to `Update`, the
/// `consume_launch_actions_system` (also in `Update`, registered
/// after the kickoff) would clear `actions` before the kickoff
/// read them, and the kickoff would observe `state = InGame` +
/// `actions.has_any() == false` — i.e. nothing to do.
///
/// Earlier versions of this PR used `.after((subview_a, subview_b, ..))`
/// — that hits Bevy 0.18's `IntoSystemSet` blanket impl, which is not
/// implemented for tuples. The set form is also more honest: the
/// kickoff observes *any* subview decision, not the four named
/// callers specifically.
pub fn register_kickoff_system(app: &mut App) {
    app.add_systems(
        EguiPrimaryContextPass,
        kickoff_world_system.after(LaunchSystemSet::Menu),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::launch::NewGameParams;
    use std::path::PathBuf;

    #[test]
    fn resolve_returns_no_action_when_state_is_not_in_game() {
        let actions = PendingLaunchActions {
            start_new_game: Some(NewGameRequest {
                params: NewGameParams::default(),
                seed: 42,
                preset: "standard".into(),
            }),
            ..Default::default()
        };
        let index = SaveIndex::empty();
        let outcome = resolve_kickoff(LaunchState::MainMenu, &actions, &index);
        assert_eq!(outcome, KickoffOutcome::NoAction);
    }

    #[test]
    fn resolve_returns_no_action_when_queue_is_empty() {
        let actions = PendingLaunchActions::default();
        let index = SaveIndex::empty();
        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &index);
        assert_eq!(outcome, KickoffOutcome::NoAction);
    }

    #[test]
    fn resolve_returns_no_action_when_only_quit_is_set() {
        // Quit is owned by the app-exit surface, not kickoff.
        let actions = PendingLaunchActions {
            quit: true,
            ..Default::default()
        };
        let index = SaveIndex::empty();
        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &index);
        assert_eq!(outcome, KickoffOutcome::NoAction);
    }

    #[test]
    fn resolve_start_new_game_uses_queued_request() {
        let request = NewGameRequest {
            params: NewGameParams::default(),
            seed: 4729103856017,
            preset: "standard".into(),
        };
        let actions = PendingLaunchActions {
            start_new_game: Some(request.clone()),
            ..Default::default()
        };
        let index = SaveIndex::empty();
        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &index);
        assert_eq!(
            outcome,
            KickoffOutcome::StartNewGame {
                request: NewGameRequest {
                    params: NewGameParams::default(),
                    seed: 4729103856017,
                    preset: "standard".into()
                }
            }
        );
    }

    #[test]
    fn resolve_load_save_uses_queued_path_with_subview_source() {
        let path = PathBuf::from("/tmp/save_alpha.ron");
        let actions = PendingLaunchActions {
            load_save: Some(path.clone()),
            ..Default::default()
        };
        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &SaveIndex::empty());
        assert_eq!(
            outcome,
            KickoffOutcome::LoadSave {
                path,
                source: KickoffLoadSource::SubviewClick
            }
        );
    }

    #[test]
    fn resolve_continue_recent_picks_first_valid_save() {
        let mut index = SaveIndex::empty();
        index.entries.push(SaveSummary::Valid {
            path: PathBuf::from("/tmp/zeta_save.ron"),
            header: crate::ui::launch::save_index::SaveHeader {
                helios_version: Some("0.4.0".into()),
                ..Default::default()
            },
        });
        index.entries.push(SaveSummary::Broken {
            path: PathBuf::from("/tmp/broken.ron"),
            error: "RON parse failed".into(),
            mtime_unix_s: None,
        });
        index.entries.push(SaveSummary::Valid {
            path: PathBuf::from("/tmp/alpha_save.ron"),
            header: crate::ui::launch::save_index::SaveHeader::default(),
        });

        let actions = PendingLaunchActions {
            continue_recent: true,
            ..Default::default()
        };

        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &index);
        // First valid entry by index order — Broken entries are
        // skipped in `most_recent_valid_save`, so the first *valid*
        // entry wins regardless of its position in the entries Vec.
        assert_eq!(
            outcome,
            KickoffOutcome::LoadSave {
                path: PathBuf::from("/tmp/zeta_save.ron"),
                source: KickoffLoadSource::ContinueRecent,
            }
        );
    }

    #[test]
    fn resolve_continue_recent_falls_through_to_start_new_game() {
        // Empty SaveIndex + queue has Continue AND a fresh game —
        // Continue can't find a save, so we fall through to the
        // New Game request.
        let actions = PendingLaunchActions {
            start_new_game: Some(NewGameRequest {
                params: NewGameParams::default(),
                seed: 0,
                preset: "casual".into(),
            }),
            continue_recent: true,
            ..Default::default()
        };

        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &SaveIndex::empty());
        assert_eq!(
            outcome,
            KickoffOutcome::StartNewGame {
                request: NewGameRequest {
                    params: NewGameParams::default(),
                    seed: 0,
                    preset: "casual".into(),
                }
            }
        );
    }

    #[test]
    fn resolve_explicit_load_save_wins_over_continue_recent() {
        let mut index = SaveIndex::empty();
        index.entries.push(SaveSummary::Valid {
            path: PathBuf::from("/tmp/old.ron"),
            header: crate::ui::launch::save_index::SaveHeader::default(),
        });
        let actions = PendingLaunchActions {
            continue_recent: true,
            load_save: Some(PathBuf::from("/tmp/explicit.ron")),
            ..Default::default()
        };

        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &index);
        assert_eq!(
            outcome,
            KickoffOutcome::LoadSave {
                path: PathBuf::from("/tmp/explicit.ron"),
                source: KickoffLoadSource::SubviewClick,
            }
        );
    }

    #[test]
    fn resolve_seed_zero_is_preserved_as_auto_sentinel() {
        let actions = PendingLaunchActions {
            start_new_game: Some(NewGameRequest {
                params: NewGameParams::default(),
                seed: 0,
                preset: "casual".into(),
            }),
            ..Default::default()
        };
        let outcome = resolve_kickoff(LaunchState::InGame, &actions, &SaveIndex::empty());
        match outcome {
            KickoffOutcome::StartNewGame { request } => {
                assert_eq!(
                    request.seed, 0,
                    "zero must round-trip as auto-roll sentinel"
                );
                assert_eq!(request.preset, "casual");
            }
            other => panic!("expected StartNewGame, got {:?}", other),
        }
    }

    #[test]
    fn most_recent_valid_save_skips_broken_entries() {
        let mut index = SaveIndex::empty();
        index.entries.push(SaveSummary::Broken {
            path: PathBuf::from("/tmp/a.ron"),
            error: "x".into(),
            mtime_unix_s: None,
        });
        index.entries.push(SaveSummary::Valid {
            path: PathBuf::from("/tmp/b.ron"),
            header: crate::ui::launch::save_index::SaveHeader::default(),
        });
        let picked = most_recent_valid_save(&index);
        assert_eq!(picked, Some(PathBuf::from("/tmp/b.ron")));
    }

    #[test]
    fn most_recent_valid_save_returns_none_when_all_broken() {
        let mut index = SaveIndex::empty();
        index.entries.push(SaveSummary::Broken {
            path: PathBuf::from("/tmp/a.ron"),
            error: "x".into(),
            mtime_unix_s: None,
        });
        assert_eq!(most_recent_valid_save(&index), None);
    }

    #[test]
    fn most_recent_valid_save_returns_none_when_empty() {
        assert_eq!(most_recent_valid_save(&SaveIndex::empty()), None);
    }

    #[test]
    fn system_consumes_sibling_actions_but_keeps_owned_field() {
        // The Bevy system wrapper clears sibling actions
        // (`continue_recent`, `load_save`) and leaves its own
        // field populated so the future GameSetup integration can
        // pick it up. We exercise that contract here via direct
        // resource mutation rather than spinning up a Bevy
        // schedule (per `feedback-egui-render-tests`).
        let mut world = bevy::ecs::world::World::new();
        world.init_resource::<LaunchState>();
        world.init_resource::<PendingLaunchActions>();
        world.insert_resource(SaveIndex::empty());

        *world.resource_mut::<LaunchState>() = LaunchState::InGame;

        let actions = PendingLaunchActions {
            start_new_game: Some(NewGameRequest {
                params: NewGameParams::default(),
                seed: 42,
                preset: "standard".into(),
            }),
            continue_recent: true,
            load_save: Some(PathBuf::from("/tmp/x.ron")),
            ..Default::default()
        };
        world.insert_resource(actions);

        // Simulate the system: clear `continue_recent` and
        // `load_save`, leave `start_new_game`.
        {
            let mut actions = world.resource_mut::<PendingLaunchActions>();
            actions.continue_recent = false;
            actions.load_save = None;
        }
        let actions = world.resource::<PendingLaunchActions>();
        assert!(actions.start_new_game.is_some(), "field left populated");
        assert!(!actions.continue_recent);
        assert!(actions.load_save.is_none());
    }
}
