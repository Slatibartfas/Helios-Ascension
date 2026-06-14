//! Auto-dismiss + click-dismiss tick system.
//!
//! Two related systems, both run in `Update` (not in the egui
//! pass), so toasts are despawned even when the player isn't
//! looking at a menu that surfaces them:
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
//!
//! Both are wrapped in a single `NotificationsSystemSet::Tick`
//! so the operator / future PRs can order them relative to the
//! coalesce (PR-D) and event bridge (PR-C) layers.

use bevy::prelude::*;

use crate::ui::notifications::components::{ActiveNotification, PendingNotificationDismissal};
use crate::ui::time::SimulationTime;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::notifications::events::NotificationSeverity;
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
            })
            .id();

        // Run a single-tick schedule.
        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(auto_dismiss_toasts);
        schedule.run(&mut world);

        // The expired toast must be despawned.
        assert!(world.get_entity(id).is_none());
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
            })
            .id();

        let mut schedule = bevy::prelude::Schedule::default();
        schedule.add_systems(auto_dismiss_toasts);
        schedule.run(&mut world);

        // Sticky — still alive.
        assert!(world.get_entity(id).is_some());
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

        assert!(world.get_entity(live).is_none());
        // Queue is drained.
        assert!(world
            .resource::<PendingNotificationDismissal>()
            .to_dismiss
            .is_empty());
    }
}
