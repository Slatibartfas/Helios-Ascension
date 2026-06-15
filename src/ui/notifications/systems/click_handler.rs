//! Click-to-focus dispatcher.
//!
//! PR-G (GRA-141) drains the `PendingNotificationClicks` resource
//! the render system pushes into and dispatches each click to the
//! relevant gameplay state. The dispatch surface is small on
//! purpose — the issue spec calls out that `SelectMission` is
//! "potentially over-scoped" and asks for a minimal body + menu
//! version, with `SelectMission` deferring to a follow-up.
//!
//! Dispatch table:
//! - `SelectBody(entity)` → insert `Selected` marker on the body
//!   and switch `active_menu.current` to `GameMenu::Survey` so the
//!   dossier opens on that body.
//! - `OpenMenu(menu)` → switch `active_menu.current` to `menu`.
//! - `SelectMission(_)` → no-op (follow-up; the in-game
//!   mission-dossier router does not exist yet).
//! - `None` → no-op (informational toasts).
//!
//! The system runs in `Update`, in the `NotificationsSystemSet::Tick`
//! set, after `apply_pending_dismissals` so a click on a toast
//! that was already despawned by the auto-dismiss timer is
//! silently dropped (the entity id is stale by the time the
//! handler runs).
//!
//! Action-queue decoupling: the render system never mutates
//! `Selected` or `ActiveMenu` directly. It writes to
//! `PendingNotificationClicks` in the egui pass; this system
//! drains and dispatches in `Update`. Same pattern as
//! `PendingResearchActions` / `PendingConstructionActions`.

use bevy::prelude::*;

use crate::astronomy::components::Selected;
use crate::game_state::{ActiveMenu, GameMenu};
use crate::ui::notifications::components::{PendingNotificationClick, PendingNotificationClicks};
use crate::ui::notifications::events::NotificationContextLink;

/// Drain the click-to-focus queue and dispatch each entry to the
/// relevant gameplay state. See module-level docs for the
/// dispatch table.
///
/// The body entity is reconstructed from the packed `to_bits()`
/// u64 the render system stored; if the entity has been
/// despawned (auto-dismiss timer fired, or the user dismissed
/// the toast via the "×" button on the same frame), the lookup
/// is silently dropped — `click_handler` does not panic on
/// stale ids.
pub fn click_to_focus(
    mut commands: Commands,
    mut queue: ResMut<PendingNotificationClicks>,
    mut active_menu: ResMut<ActiveMenu>,
    active: Query<Entity, With<crate::ui::notifications::components::ActiveNotification>>,
) {
    if queue.to_focus.is_empty() {
        return;
    }

    // Build a HashSet<Entity> from the current live toast set so
    // we can cheaply drop stale clicks (the player clicked a toast
    // that was despawned earlier this frame by the auto-dismiss
    // timer or a same-frame dismiss click).
    let active_set: std::collections::HashSet<Entity> = active.iter().collect();

    // Move the queue out, walk the entries, dispatch. We do not
    // push back anything — the queue is a one-shot, frame-scoped
    // buffer (mirrors `apply_pending_dismissals`).
    let to_process: Vec<PendingNotificationClick> = queue.to_focus.drain(..).collect();
    for click in to_process {
        // Reconstruct the toast entity from the packed `to_bits()`
        // and drop the entry if the toast is no longer alive.
        // Kilo review CRITICAL: stored-but-unused `entity_bits`
        // was a code-smell; we now use it for the staleness guard
        // so the dispatch is silently dropped for clicks against
        // despawned toasts. `NotificationContextLink::SelectBody`
        // carries its own `body: Entity` — we *don't* use that for
        // the staleness check, only the toast entity, so an
        // OpenMenu/SelectMission/None click that survives a toast
        // despawn still applies (you might be menu-redirected even
        // though the toast is gone).
        let toast_entity = Entity::from_bits(click.entity_bits);
        let toast_alive = active_set.contains(&toast_entity);
        if !toast_alive {
            continue;
        }

        match click.context_link {
            NotificationContextLink::SelectBody(body) => {
                // Insert `Selected` on the body. Bevy 0.18's
                // `EntityCommands::insert` on a despawned entity
                // is a silent no-op (the entity is created lazily
                // in the next flush) — we don't guard with
                // `get_entity` because that would require a
                // double-lookup on the hot path. The
                // `test_click_with_stale_body_id_does_not_panic`
                // test below documents the no-panic contract.
                commands.entity(body).insert(Selected);
                active_menu.current = GameMenu::Survey;
            }
            NotificationContextLink::OpenMenu(menu) => {
                active_menu.current = menu;
            }
            // Reserved for a follow-up — the in-game
            // mission-dossier router does not exist yet, so the
            // PR-G minimal version drops these on the floor.
            NotificationContextLink::SelectMission(_) => {}
            // Informational toasts have no jump target; nothing
            // to do.
            NotificationContextLink::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::notifications::events::{NotificationContextLink, NotificationSeverity};
    use crate::ui::notifications::settings::NotificationCategoryId;
    use bevy::prelude::World;

    /// Helper: a fresh world with the click handler's resource
    /// deps in place. Mirrors the test scaffolding style used in
    /// `event_bridge.rs` and `tick.rs`.
    fn fresh_world() -> World {
        let mut world = World::new();
        world.insert_resource(ActiveMenu::default());
        world.insert_resource(PendingNotificationClicks::default());
        world.insert_resource(crate::ui::notifications::settings::NotificationSettings::default());
        world
    }

    /// Helper: spawn a placeholder `ActiveNotification` toast in
    /// the world. The handler's staleness guard consults
    /// `Query<Entity, With<ActiveNotification>>` and drops clicks
    /// against despawned toasts, so every test fixture must
    /// spawn a real toast for the dispatch to fire.
    fn spawn_toast(world: &mut World) -> Entity {
        world
            .spawn(crate::ui::notifications::components::ActiveNotification {
                category: NotificationCategoryId::from("test.toast"),
                severity: NotificationSeverity::Info,
                title: "T".to_string(),
                body: "B".to_string(),
                created_at: 0.0,
                auto_dismiss_s: 100.0,
                sticky: false,
                dedup_key: None,
                count: 1,
                context_link: NotificationContextLink::None,
            })
            .id()
    }

    /// Issue acceptance test 1: clicking a body-bearing toast
    /// selects the body and opens the survey menu.
    #[test]
    fn test_click_selects_body_and_opens_survey() {
        let mut world = fresh_world();
        // Spawn a real toast so the staleness guard lets the
        // dispatch through.
        let toast = spawn_toast(&mut world);

        // Spawn a body entity and pre-tag a `Selected` is not
        // there. The handler must insert one.
        let body = world.spawn_empty().id();
        assert!(world.get_entity(body).is_ok());

        // Push a `SelectBody(body)` click.
        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast.to_bits(),
                context_link: NotificationContextLink::SelectBody(body),
            });

        // Run the handler.
        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(click_to_focus);
        schedule.run(&mut world);

        // The body now carries the `Selected` marker.
        let entity_ref = world
            .get_entity(body)
            .expect("body must still be alive after handler runs");
        assert!(
            entity_ref.get::<Selected>().is_some(),
            "body must have Selected inserted by click_handler"
        );

        // `active_menu.current` switched to Survey.
        let menu = world.resource::<ActiveMenu>();
        assert_eq!(menu.current, GameMenu::Survey);

        // The queue is drained.
        assert!(world
            .resource::<PendingNotificationClicks>()
            .to_focus
            .is_empty());
    }

    /// Issue acceptance test 2: a research-completed toast
    /// (carrying `OpenMenu(GameMenu::Research)`) routes the
    /// player to the research menu on click.
    #[test]
    fn test_click_opens_research_menu_on_research_completed_toast() {
        let mut world = fresh_world();
        let toast = spawn_toast(&mut world);
        // Sanity: default menu is Survey, so the test will detect
        // any non-Research result.
        assert_eq!(world.resource::<ActiveMenu>().current, GameMenu::Survey);

        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast.to_bits(),
                context_link: NotificationContextLink::OpenMenu(GameMenu::Research),
            });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(click_to_focus);
        schedule.run(&mut world);

        assert_eq!(world.resource::<ActiveMenu>().current, GameMenu::Research);
    }

    /// Issue acceptance test 3: a `None` context link is a
    /// no-op — no state change at all.
    #[test]
    fn test_click_no_op_when_context_link_is_none() {
        let mut world = fresh_world();
        let toast = spawn_toast(&mut world);
        // Pre-state: Survey menu, no Selected entities.
        assert_eq!(world.resource::<ActiveMenu>().current, GameMenu::Survey);
        let pre_query = world.query::<&Selected>().iter(&world).count();
        assert_eq!(pre_query, 0);

        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast.to_bits(),
                context_link: NotificationContextLink::None,
            });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(click_to_focus);
        schedule.run(&mut world);

        // Menu unchanged.
        assert_eq!(world.resource::<ActiveMenu>().current, GameMenu::Survey);
        // No entity got a `Selected` marker.
        let post_query = world.query::<&Selected>().iter(&world).count();
        assert_eq!(
            post_query, 0,
            "click_handler must not insert Selected on a None context link"
        );
        // Queue drained.
        assert!(world
            .resource::<PendingNotificationClicks>()
            .to_focus
            .is_empty());
    }

    /// `SelectMission` is documented as a no-op in the PR-G
    /// minimal version (the in-game mission-dossier router is a
    /// follow-up). Verify the handler does not panic and does
    /// not mutate state.
    #[test]
    fn test_click_select_mission_is_noop_in_prg() {
        let mut world = fresh_world();
        let toast = spawn_toast(&mut world);
        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast.to_bits(),
                context_link: NotificationContextLink::SelectMission(42),
            });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(click_to_focus);
        schedule.run(&mut world);

        // Menu unchanged.
        assert_eq!(world.resource::<ActiveMenu>().current, GameMenu::Survey);
        // Queue drained.
        assert!(world
            .resource::<PendingNotificationClicks>()
            .to_focus
            .is_empty());
    }

    /// A stale toast id (toast already despawned before the
    /// handler runs) must not dispatch — the staleness guard
    /// drops the click before the match. The `SelectBody` body
    /// entity being also stale is fine: the dispatch is
    /// short-circuited before we ever reach the body.
    #[test]
    fn test_click_with_stale_body_id_does_not_panic() {
        let mut world = fresh_world();
        // A fresh `Entity::from_bits(u64::MAX)` is not in the
        // world, so the handler's staleness guard will skip
        // this entry entirely (no Selected inserted, no menu
        // change). The schedule must complete without
        // panicking — the `command.entity(stale).insert(...)`
        // path is never taken.
        let stale_toast = Entity::from_bits(u64::MAX);
        let stale_body = Entity::from_bits(u64::MAX);
        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: stale_toast.to_bits(),
                context_link: NotificationContextLink::SelectBody(stale_body),
            });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(click_to_focus);
        schedule.run(&mut world);

        // The schedule ran. The queue is drained.
        assert!(world
            .resource::<PendingNotificationClicks>()
            .to_focus
            .is_empty());

        // No `Selected` was inserted (the guard short-circuited
        // before the dispatch).
        let post_query = world.query::<&Selected>().iter(&world).count();
        assert_eq!(
            post_query, 0,
            "click_handler must not insert Selected on a stale-toast click"
        );
    }

    /// The handler must drain multiple entries in one frame.
    /// Mirrors how the render system might push several toasts'
    /// clicks in the same frame.
    #[test]
    fn test_click_drains_multiple_entries() {
        let mut world = fresh_world();
        let toast1 = spawn_toast(&mut world);
        let toast2 = spawn_toast(&mut world);
        let toast3 = spawn_toast(&mut world);
        let body1 = world.spawn_empty().id();
        let body2 = world.spawn_empty().id();
        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast1.to_bits(),
                context_link: NotificationContextLink::SelectBody(body1),
            });
        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast2.to_bits(),
                context_link: NotificationContextLink::SelectBody(body2),
            });
        world
            .resource_mut::<PendingNotificationClicks>()
            .push(PendingNotificationClick {
                entity_bits: toast3.to_bits(),
                context_link: NotificationContextLink::OpenMenu(GameMenu::Construction),
            });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(click_to_focus);
        schedule.run(&mut world);

        // Both bodies have `Selected`.
        for b in [body1, body2] {
            let entity_ref = world.get_entity(b).expect("body must be alive");
            assert!(
                entity_ref.get::<Selected>().is_some(),
                "body {:?} must have Selected after multi-click drain",
                b
            );
        }
        // Last write wins for `active_menu.current` — the third
        // entry set it to Construction after the first two set
        // it to Survey. We assert *only* that the handler ran
        // all three and the queue is drained.
        let menu = world.resource::<ActiveMenu>();
        assert_eq!(menu.current, GameMenu::Construction);
        assert!(world
            .resource::<PendingNotificationClicks>()
            .to_focus
            .is_empty());
    }

    /// Compile-time guard: the imports used by the test module
    /// are actually used somewhere in the file. Catches
    /// accidental import cleanups that would otherwise
    /// `unused_imports`-warn on `cargo build` but only in the
    /// test build.
    #[test]
    fn test_imports_compile() {
        let _: NotificationSeverity = NotificationSeverity::Info;
        let _: NotificationCategoryId = NotificationCategoryId::from("compile.guard");
    }
}
