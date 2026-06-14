//! Auto-dismiss + click-dismiss + pause-on-event tick systems.
//!
//! Three related systems, all in `Update` (not the egui pass), so
//! toasts are despawned and the simulation can be paused even when
//! the player isn't looking at a menu that surfaces them:
//!
//! - `auto_dismiss_toasts` — walks `ActiveNotification`
//!   entities, despawns any whose `auto_dismiss_s` has elapsed
//!   (unless `sticky`). Time source is
//!   `SimulationTime::elapsed_seconds()`, never
//!   `Time<Virtual>`, per the CTO design.
//! - `apply_pending_dismissals` — drains the
//!   `PendingNotificationDismissal` queue the render system
//!   pushes to on click. Same action-queue decoupling as
//!   `PendingResearchActions` / `PendingConstructionActions`.
//! - `pause_on_event_toasts` — PR-F (GRA-140). For any
//!   `ActiveNotification` whose category requests
//!   `pause_on_event: true`, calls `TimeScale::pause()` once
//!   per frame (the first new toast in a frame is enough;
//!   subsequent toasts in the same frame are no-ops per the
//!   "5 events paused the game 5 times" trap in the design
//!   comment).
//!
//! All three are wrapped in `NotificationsSystemSet::Tick` so the
//! coalesce (PR-D) and event bridge (PR-C) layers can be ordered
//! before them. **Ordering note (PR-F):** `Coalesce` must run
//! before `Tick` because Coalesce inserts new `ActiveNotification`
//! entities (or increments `count` on existing ones). Tick only
//! sees the *newly inserted* entities via the per-system
//! `Local<HashSet<Entity>>` cache, so if Tick runs first the new
//! entity isn't there yet and `pause()` never fires.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::ui::notifications::components::{ActiveNotification, PendingNotificationDismissal};
use crate::ui::notifications::data::NotificationCategoriesData;
use crate::ui::notifications::settings::NotificationSettings;
use crate::ui::time::{SimulationTime, TimeScale};

/// Run the auto-dismiss timer.
///
/// `auto_dismiss_s <= 0.0` and `f32::INFINITY` are both treated
/// as "no auto-dismiss" — the despawn loop skips them. Sticky
/// toasts also skip the timer.
pub fn auto_dismiss_toasts(
    sim_time: Res<SimulationTime>,
    mut commands: Commands,
    active: Query<(Entity, &ActiveNotification)>,
) {
    let now = sim_time.elapsed_seconds();
    for (entity, n) in &active {
        if n.sticky {
            continue;
        }
        if !n.auto_dismiss_s.is_finite() || n.auto_dismiss_s <= 0.0 {
            continue;
        }
        let elapsed = (now - n.created_at).max(0.0) as f32;
        if elapsed >= n.auto_dismiss_s {
            commands.entity(entity).despawn();
        }
    }
}

/// Drain the click-to-dismiss queue and despawn the listed
/// entities. The render system pushes the entity's packed
/// `to_bits()` u64 into the queue; here we look it up against
/// the current entity set and despawn. Stale ids (the entity
/// already despawned by the timer) are silently dropped.
pub fn apply_pending_dismissals(
    mut commands: Commands,
    mut queue: ResMut<PendingNotificationDismissal>,
    active: Query<Entity>,
) {
    if queue.to_dismiss.is_empty() {
        return;
    }

    // Build a HashSet<Entity> from the current active set so the
    // lookup is O(1) per id and stale ids are skipped cheaply.
    let active_set: std::collections::HashSet<Entity> = active.iter().collect();

    // Move the queue out, walk the entries, despawn the
    // entities that are still alive. We do not push back
    // anything — the queue is a one-shot, frame-scoped buffer.
    let to_process: Vec<u64> = queue.to_dismiss.drain(..).collect();
    for id_bits in to_process {
        // u64 → Entity via `from_bits` (the inverse of
        // `to_bits`). Stale or malformed ids produce a fresh
        // entity which won't be in the active set, so the
        // `if let` guard catches them.
        let entity = Entity::from_bits(id_bits);
        if active_set.contains(&entity) {
            commands.entity(entity).despawn();
        }
    }
}

/// PR-F (GRA-140): pause the simulation on a freshly-inserted
/// toast whose category requests `pause_on_event`.
///
/// The system remembers the entity set seen on the previous tick
/// in a `Local<HashSet<Entity>>`. Anything present this tick but
/// not in that set is a *newly inserted* entity (either spawned
/// by the bridge in PR-C or coalesced from a duplicate event in
/// PR-D). For each new entity, the category is resolved against
/// `NotificationSettings` (override wins) and the manifest
/// `pause_on_event` field. The first one in a frame whose
/// resolved flag is `true` triggers `TimeScale::pause()`;
/// subsequent toasts in the same frame are no-ops because
/// `is_paused()` is already `true`.
///
/// Resume is manual (the existing space-bar / play button path is
/// unchanged). The auto-resume timer the original design
/// considered was dropped — surprising the player was the
/// concern.
pub fn pause_on_event_toasts(
    mut time_scale: ResMut<TimeScale>,
    settings: Res<NotificationSettings>,
    categories: Res<NotificationCategoriesData>,
    active: Query<(Entity, &ActiveNotification)>,
    mut seen_last_tick: Local<HashSet<Entity>>,
) {
    if time_scale.is_paused() {
        // Even if we're already paused, refresh the cache so the
        // next frame's diff is correct. Otherwise an entity that
        // was inserted mid-paused-this-frame would be missed on
        // resume.
        refresh_seen_set(&active, &mut seen_last_tick);
        return;
    }

    let current: HashSet<Entity> = active.iter().map(|(e, _)| e).collect();

    // Walk only the *new* entities — anything not in
    // `seen_last_tick`. The iteration order is the active set's
    // storage order; we just need the first one to fire.
    for (entity, toast) in &active {
        if seen_last_tick.contains(&entity) {
            continue;
        }

        // Resolve the category's pause-on-event flag. Settings
        // override wins; fall back to the manifest row; if the
        // category has been removed from the manifest entirely
        // (shouldn't happen — spawns only fire for categories
        // present at startup), default to no-pause.
        let manifest_default = categories
            .get(&toast.category)
            .map(|c| c.pause_on_event)
            .unwrap_or(false);
        if settings.is_category_pause_on_event(&toast.category, manifest_default) {
            time_scale.pause();
            // Stop scanning — the design's "5 events paused the
            // game 5 times" trap means we want exactly one
            // `pause()` call per frame.
            break;
        }
    }

    // Refresh the cache for the next tick.
    *seen_last_tick = current;
}

fn refresh_seen_set(active: &Query<(Entity, &ActiveNotification)>, cache: &mut HashSet<Entity>) {
    cache.clear();
    cache.extend(active.iter().map(|(e, _)| e));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::notifications::events::{NotificationContextLink, NotificationSeverity};
    use crate::ui::notifications::settings::NotificationCategoryId;
    use crate::ui::time::SimulationTime;

    /// Build a SimulationTime with `elapsed_seconds()` returning
    /// the given offset, no matter what the real `Time` says.
    /// `SimulationTime::elapsed` is the single source for the
    /// tick system, so we manipulate that directly.
    fn sim_time_at(seconds: f64) -> SimulationTime {
        let mut t = SimulationTime::default();
        t.elapsed = seconds;
        t
    }

    /// Auto-dismiss fires when the timer has elapsed.
    /// Issue acceptance: "spawn an ActiveNotification with
    /// auto_dismiss_s: 0.1 and a SimulationTime::elapsed already
    /// past, run tick, assert despawn".
    #[test]
    fn test_tick_dismisses_expired_toasts() {
        let mut world = bevy::prelude::World::new();
        world.insert_resource(sim_time_at(100.0));

        // Spawn a toast that was created at t=0 with
        // auto_dismiss_s=0.1. The current sim time is 100s, so
        // it has long expired.
        let id = world
            .spawn(ActiveNotification {
                category: NotificationCategoryId::from("test.expired"),
                severity: NotificationSeverity::Info,
                title: "Expired".to_string(),
                body: String::new(),
                created_at: 0.0,
                auto_dismiss_s: 0.1,
                sticky: false,
                dedup_key: None,
                count: 1,
                context_link: NotificationContextLink::None,
            })
            .id();

        // Run a single-tick schedule.
        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(auto_dismiss_toasts);
        schedule.run(&mut world);

        // The expired toast must be despawned. Bevy 0.18's
        // `World::get_entity` returns `Result<EntityRef, _>`
        // (not `Option`); the Ok variant means the entity is
        // still alive.
        assert!(world.get_entity(id).is_err());
    }

    /// Sticky toasts ignore the auto-dismiss timer.
    #[test]
    fn test_tick_skips_sticky_toasts() {
        let mut world = bevy::prelude::World::new();
        world.insert_resource(sim_time_at(100.0));

        let id = world
            .spawn(ActiveNotification {
                category: NotificationCategoryId::from("test.sticky"),
                severity: NotificationSeverity::Critical,
                title: "Sticky".to_string(),
                body: String::new(),
                created_at: 0.0,
                auto_dismiss_s: 0.1,
                sticky: true,
                dedup_key: None,
                count: 1,
                context_link: NotificationContextLink::None,
            })
            .id();

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(auto_dismiss_toasts);
        schedule.run(&mut world);

        // Sticky — still alive. Bevy 0.18 Result variant.
        assert!(world.get_entity(id).is_ok());
    }

    /// Click-to-dismiss queue: an entry whose entity is still
    /// alive gets despawned; a stale id is silently dropped.
    #[test]
    fn test_apply_pending_dismissals_drains_queue_and_despawns() {
        let mut world = bevy::prelude::World::new();
        world.insert_resource(PendingNotificationDismissal::default());

        let live = world
            .spawn(ActiveNotification {
                category: NotificationCategoryId::from("test.click"),
                severity: NotificationSeverity::Info,
                title: "Click me".to_string(),
                body: String::new(),
                created_at: 0.0,
                auto_dismiss_s: 100.0,
                sticky: false,
                dedup_key: None,
                count: 1,
                context_link: NotificationContextLink::None,
            })
            .id();

        // Live id and a stale id (a fresh, never-spawned entity).
        let stale = Entity::from_bits(u64::MAX);
        world
            .resource_mut::<PendingNotificationDismissal>()
            .push(live.to_bits());
        world
            .resource_mut::<PendingNotificationDismissal>()
            .push(stale.to_bits());

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(apply_pending_dismissals);
        schedule.run(&mut world);

        assert!(world.get_entity(live).is_err());
        // Queue is drained.
        assert!(world
            .resource::<PendingNotificationDismissal>()
            .to_dismiss
            .is_empty());
    }

    // PR-F (GRA-140) tests. These exercise the pause-on-event
    // wiring without spinning up a render pass: the system reads
    // `TimeScale`, `NotificationSettings`, and
    // `NotificationCategoriesData` resources, plus a query of
    // `ActiveNotification` entities. `World::new()` +
    // resource inserts is enough; no `EguiContexts`, no
    // `DefaultPlugins`, no `App` build. See
    // `feedback-egui-render-tests` for the rationale.

    /// Build a `TimeScale` at the given non-zero speed.
    fn time_scale_at(speed: f32) -> TimeScale {
        let mut ts = TimeScale::new();
        ts.set_speed(speed);
        ts
    }

    /// Build a `NotificationCategoriesData` with a single
    /// `survey.mission_failed` row that has `pause_on_event: true`.
    fn categories_with_pause() -> NotificationCategoriesData {
        use crate::ui::notifications::data::{NotificationCategoriesData, NotificationCategory};
        use crate::ui::notifications::settings::NotificationCategoryId;
        let mut data = NotificationCategoriesData::default();
        data.categories.insert(
            NotificationCategoryId::from("survey.mission_failed"),
            NotificationCategory {
                id: "survey.mission_failed".to_string(),
                display_name: "Mission failed".to_string(),
                default_dismiss_s: 6.0,
                enabled: true,
                pause_on_event: true,
            },
        );
        data
    }

    /// PR-F issue acceptance #1: a fresh pause-on-event toast
    /// in a not-already-paused game triggers `TimeScale::pause()`,
    /// and the pre-pause speed is captured in `last_active_scale`.
    ///
    /// The system uses a `Local<HashSet<Entity>>` cache to detect
    /// "newly inserted" entities. Running the schedule once
    /// against a world that already contains the toast counts
    /// the toast as a fresh insert (the Local starts empty),
    /// which is exactly the first-frame behaviour we want to
    /// test: a real bridge would call `world.spawn(...)` mid-frame
    /// and Tick runs the same frame.
    #[test]
    fn test_pause_on_event_pauses_when_not_already_paused() {
        use crate::ui::notifications::settings::NotificationCategoryId;

        let mut world = bevy::prelude::World::new();
        world.insert_resource(time_scale_at(10.0));
        world.insert_resource(NotificationSettings::default());
        world.insert_resource(categories_with_pause());
        // SimulationTime is unused by pause_on_event_toasts but
        // init_resource pattern requires a clean world, and the
        // system doesn't read it; skip.

        world.spawn(ActiveNotification {
            category: NotificationCategoryId::from("survey.mission_failed"),
            severity: NotificationSeverity::Warning,
            title: "Mission failed".to_string(),
            body: String::new(),
            created_at: 0.0,
            auto_dismiss_s: 6.0,
            sticky: false,
            dedup_key: None,
            count: 1,
        });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(pause_on_event_toasts);
        schedule.run(&mut world);

        let ts = world.resource::<TimeScale>();
        assert!(ts.is_paused(), "TimeScale must be paused after the tick");
        assert!(
            (ts.last_active_scale() - 10.0).abs() < f32::EPSILON,
            "last_active_scale must capture the pre-pause speed (10.0), got {}",
            ts.last_active_scale()
        );
        assert!(
            ts.scale == 0.0,
            "current scale must be 0.0 (paused), got {}",
            ts.scale
        );
    }

    /// PR-F issue acceptance #2: when the game is already paused,
    /// a new pause-on-event toast is a no-op — `last_active_scale`
    /// stays at whatever it was (here, the constructor's default
    /// 3_600.0 since we never called `set_speed`).
    #[test]
    fn test_pause_on_event_is_noop_when_already_paused() {
        use crate::ui::notifications::settings::NotificationCategoryId;

        let mut world = bevy::prelude::World::new();
        let mut ts = TimeScale::new();
        // Pause once with a known pre-pause speed, then poke the
        // captured value to a non-default number so we can
        // assert it didn't change.
        ts.set_speed(7.5);
        ts.pause();
        let captured = ts.last_active_scale();
        world.insert_resource(ts);
        world.insert_resource(NotificationSettings::default());
        world.insert_resource(categories_with_pause());

        world.spawn(ActiveNotification {
            category: NotificationCategoryId::from("survey.mission_failed"),
            severity: NotificationSeverity::Warning,
            title: "Mission failed".to_string(),
            body: String::new(),
            created_at: 0.0,
            auto_dismiss_s: 6.0,
            sticky: false,
            dedup_key: None,
            count: 1,
        });

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(pause_on_event_toasts);
        schedule.run(&mut world);

        let ts_after = world.resource::<TimeScale>();
        assert!(ts_after.is_paused());
        assert!(
            (ts_after.last_active_scale() - captured).abs() < f32::EPSILON,
            "last_active_scale must be unchanged when the system no-ops, \
             was {} now {}",
            captured,
            ts_after.last_active_scale()
        );
    }
}
