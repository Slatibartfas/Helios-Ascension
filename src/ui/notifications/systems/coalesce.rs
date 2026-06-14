//! Coalesce layer: dedup repeated `NotificationEvent`s into a
//! single `ActiveNotification` entity per (category, dedup_key)
//! group, within the global `default_group_window_s` window.
//!
//! Ordering: this system runs in `Update` (no egui), in the
//! `NotificationsSystemSet::Coalesce` set, chained before
//! `NotificationsSystemSet::Tick` (PR-B) so the tick system sees
//! the latest `count`/`created_at` when computing auto-dismiss.
//!
//! # Behavior
//!
//! For every `NotificationEvent` drained in the current frame:
//!
//! 1. If the global `global_enabled` flag is `false`, drop the
//!    event silently.
//! 2. If the event's category is disabled in either the manifest
//!    (`NotificationCategoriesData`) or the per-category player
//!    override, drop the event silently.
//! 3. If the event has no `dedup_key`, always spawn a fresh
//!    entity.
//! 4. Otherwise, scan existing `ActiveNotification` entities. If
//!    one matches (same `category`, same `dedup_key`, `created_at`
//!    within the global `default_group_window_s`) and the
//!    existing entity's severity rank is `>=` the incoming
//!    event's rank, bump its `count` and refresh `created_at`.
//!    If the existing entity's severity rank is *lower* (e.g. an
//!    `Info` is on screen and a `Critical` with the same key
//!    fires), the `Critical` replaces it: count resets to 1,
//!    severity upgrades, `created_at` resets.
//!
//! # Group window
//!
//! PR-A only models a single global `default_group_window_s` on
//! `NotificationSettings`. Per-category group windows land in
//! a follow-up if LGD wants them.
//!
//! # Risk: coalesce-before-tick
//!
//! If a toast has just expired and the new one fires in the same
//! frame, the despawn runs *after* coalesce inserts the new one —
//! the visual is correct (the new toast is on screen the whole
//! frame, the expired one is gone next frame). Documented in the
//! system set comment.

use bevy::prelude::*;

use crate::ui::time::SimulationTime;

use super::super::components::ActiveNotification;
use super::super::data::NotificationCategoriesData;
use super::super::events::{NotificationEvent, NotificationSeverity};
use super::super::settings::{NotificationCategoryId, NotificationSettings};
// `NotificationsSystemSet` is defined in `super` (PR-B GRA-136 owns
// the canonical enum so PR-C / D / E share one set definition).

/// Local ranking for severity comparisons. Higher rank = louder /
/// more attention-worthy. Mirrors the order the spec uses for
/// "Critical replaces Info" — the enum doesn't derive `PartialOrd`
/// (PR-A kept it `Eq` only to avoid accidental `Info < Info == true`
/// surprises), so we map explicitly here.
fn severity_rank(s: NotificationSeverity) -> u8 {
    match s {
        NotificationSeverity::Info => 0,
        NotificationSeverity::Notice => 1,
        NotificationSeverity::Warning => 2,
        NotificationSeverity::Critical => 3,
    }
}

/// Resolve the manifest default for a category. The coalesce
/// system runs in `Update` (post-Startup) so the resource is
/// always present in production. In tests we fall back to
/// `enabled = true, default_dismiss_s = 0.0` if the manifest is
/// missing or the category is unknown — matches the RON
/// `default_enabled` serde default.
fn manifest_defaults(
    categories: &NotificationCategoriesData,
    id: &NotificationCategoryId,
) -> (bool, f32) {
    match categories.get(id) {
        Some(cat) => (cat.enabled, cat.default_dismiss_s),
        None => (true, 0.0),
    }
}

/// Coalesce system. Drains `NotificationEvent`s from the message
/// buffer and produces `ActiveNotification` entities.
pub fn coalesce_notifications(
    mut commands: Commands,
    mut events: MessageReader<NotificationEvent>,
    settings: Res<NotificationSettings>,
    categories: Res<NotificationCategoriesData>,
    time: Res<SimulationTime>,
    mut active: Query<(Entity, &mut ActiveNotification)>,
) {
    // Materialize events up front so the inner loop can both
    // mutate `active` and look up by entity. Bevy 0.18 forbids
    // holding a `MessageReader` borrow while a `Query` borrow is
    // live in the same scope.
    let events: Vec<NotificationEvent> = events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    if !settings.global_enabled {
        return;
    }

    let now = time.elapsed_seconds();
    let group_window = settings.default_group_window_s;

    // Local index: snapshot of every live toast at the start of
    // the frame. We mutate `active` in place but also keep this
    // in sync so a second event in the same frame finds the
    // fresh `created_at` / severity. Without the index, a
    // repeat event arriving in the same frame would re-spawn
    // because the snapshot was stale.
    let mut index: Vec<ActiveRef> = Vec::with_capacity(active.iter().len());
    for (entity, note) in &mut active {
        index.push(ActiveRef {
            entity,
            category: note.category.clone(),
            dedup_key: note.dedup_key.clone(),
            severity_rank: severity_rank(note.severity),
            created_at: note.created_at,
        });
    }

    for event in &events {
        // Per-category enabled check. Combines the manifest's
        // `enabled` row (from `assets/data/notifications.ron`)
        // with the player's per-category override in
        // `NotificationSettings`; either being off drops the
        // event. Unknown categories default to enabled (matches
        // the RON `serde(default)` rule for new fields).
        let (manifest_enabled, manifest_default_dismiss_s) =
            manifest_defaults(&categories, &event.category);
        if !settings.is_category_enabled(&event.category, manifest_enabled) {
            continue;
        }

        let in_severity = severity_rank(event.severity);
        // Auto-dismiss fallback: prefer the manifest's
        // `default_dismiss_s` over the group window. Previously
        // the group window (default 2.0 s) was reused as the
        // auto-dismiss fallback, which made sticky categories
        // (manifest `default_dismiss_s = 0.0`) auto-dismiss in
        // 2.0 s instead of staying sticky. Kilo finding.
        let auto_dismiss_fallback = manifest_default_dismiss_s;

        // No dedup key → always spawn.
        let Some(key) = event.dedup_key.as_ref() else {
            let new_entity = spawn_new(&mut commands, event, &time, auto_dismiss_fallback);
            index.push(ActiveRef {
                entity: new_entity,
                category: event.category.clone(),
                dedup_key: None,
                severity_rank: in_severity,
                created_at: now,
            });
            continue;
        };

        // Find a matching live entity: same category + same key +
        // created within the group window.
        let match_idx = index.iter().position(|r| {
            r.category == event.category
                && r.dedup_key.as_deref() == Some(key.as_str())
                && (now - r.created_at) <= group_window as f64
        });

        if let Some(idx) = match_idx {
            let matched = index[idx].entity;
            if index[idx].severity_rank >= in_severity {
                // Bump branch: count++, refresh created_at.
                if let Ok((_, mut note)) = active.get_mut(matched) {
                    note.count = note.count.saturating_add(1);
                    note.created_at = now;
                }
                index[idx].created_at = now;
            } else {
                // Severity upgrade: replace in place. Reset
                // count, upgrade severity, refresh created_at,
                // and update title/body from the new event.
                if let Ok((_, mut note)) = active.get_mut(matched) {
                    note.count = 1;
                    note.severity = event.severity;
                    note.title = event.title.clone();
                    note.body = event.body.clone();
                    note.auto_dismiss_s = event.auto_dismiss_s.unwrap_or(auto_dismiss_fallback);
                    note.sticky = event.sticky;
                    note.created_at = now;
                }
                index[idx].severity_rank = in_severity;
                index[idx].created_at = now;
            }
        } else {
            // New group: spawn a fresh entity and add it to the
            // local index so subsequent events in the same
            // frame can coalesce with it.
            let new_entity = spawn_new(&mut commands, event, &time, auto_dismiss_fallback);
            index.push(ActiveRef {
                entity: new_entity,
                category: event.category.clone(),
                dedup_key: event.dedup_key.clone(),
                severity_rank: in_severity,
                created_at: now,
            });
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveRef {
    entity: Entity,
    category: NotificationCategoryId,
    dedup_key: Option<String>,
    severity_rank: u8,
    created_at: f64,
}

fn spawn_new(
    commands: &mut Commands,
    event: &NotificationEvent,
    time: &SimulationTime,
    default_dismiss_s: f32,
) -> Entity {
    commands
        .spawn(ActiveNotification::from_event(
            event,
            time,
            default_dismiss_s,
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::notifications::components::PendingNotificationDismissal;
    use crate::ui::notifications::events::NotificationContextLink;

    /// Build a `NotificationSettings` resource with the given
    /// `default_group_window_s` override (or `None` to keep the
    /// `Default::default()` 2.0 s). Always enables the category.
    fn settings_with_window(group_window: Option<f32>) -> NotificationSettings {
        let mut s = NotificationSettings::default();
        if let Some(w) = group_window {
            s.default_group_window_s = w;
        }
        // Force the per-category override to `enabled: true` so
        // coalesce doesn't drop the events (the default empty map
        // + the `manifest_default_enabled=true` arg in
        // `is_category_enabled` already gives us this, but we
        // set it explicitly to make the test intent obvious).
        // `manifest_default_dismiss_s = 5.0` matches the test
        // manifest's default; the Kilo-fixed `get_or_default`
        // (merged via PR #169) now requires a third arg.
        let _ = s.get_or_default(&"survey.mission_complete".into(), true, 5.0);
        s
    }

    /// Build a `NotificationEvent` with the given dedup_key. The
    /// body is intentionally short — coalesce doesn't read it.
    fn event(
        category: &str,
        severity: NotificationSeverity,
        key: Option<&str>,
    ) -> NotificationEvent {
        NotificationEvent {
            category: category.into(),
            severity,
            title: format!("{category} fired"),
            body: String::new(),
            dedup_key: key.map(|s| s.to_string()),
            auto_dismiss_s: None,
            sticky: false,
            context_link: NotificationContextLink::None,
        }
    }

    /// Build Bevy `App` with everything the coalesce system needs.
    /// Does NOT add `NotificationsPlugin` — that would also add
    /// the RON loader and the Bevy `Reflect` machinery, neither
    /// of which the coalesce system touches. The categories
    /// manifest is provided inline so tests stay self-contained.
    fn build_app(group_window: Option<f32>) -> App {
        let mut app = App::new();
        app.init_resource::<SimulationTime>();
        app.init_resource::<PendingNotificationDismissal>();
        app.init_resource::<NotificationCategoriesData>();
        app.add_message::<NotificationEvent>();
        app.insert_resource(settings_with_window(group_window));
        app.add_systems(Update, coalesce_notifications);
        app
    }

    fn count_active(app: &mut App) -> usize {
        let mut q = app.world_mut().query::<&ActiveNotification>();
        q.iter(app.world()).count()
    }

    #[test]
    fn test_coalesce_groups_repeated_event() {
        let mut app = build_app(Some(2.0));

        // 3 identical events at t=0.0, 0.5, 1.0 — all within 1.5 s
        // and within the 2.0 s default group window.
        for t in [0.0_f64, 0.5, 1.0] {
            app.world_mut().resource_mut::<SimulationTime>().elapsed = t;
            app.world_mut().write_message(event(
                "survey.mission_complete",
                NotificationSeverity::Notice,
                Some("mare-imbrium-1"),
            ));
            app.update();
        }

        assert_eq!(
            count_active(&mut app),
            1,
            "3 events within 1.5 s should coalesce into 1 entity"
        );

        // Find the entity and assert count == 3.
        let mut q = app.world_mut().query::<&ActiveNotification>();
        let note = q.iter(app.world()).next().unwrap();
        assert_eq!(note.count, 3, "count should be 3 after 3 merges");
        // created_at is refreshed to the last event's time.
        assert!((note.created_at - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_coalesce_does_not_group_across_window() {
        let mut app = build_app(Some(2.0));

        // 3 events spaced 3 s apart with a 2.0 s group window.
        // Each event lands outside the previous one's window
        // (Δt = 3.0 > 2.0), so each spawns its own entity.
        for t in [0.0_f64, 3.0, 6.0] {
            app.world_mut().resource_mut::<SimulationTime>().elapsed = t;
            app.world_mut().write_message(event(
                "survey.mission_complete",
                NotificationSeverity::Notice,
                Some("mare-imbrium-1"),
            ));
            app.update();
        }

        assert_eq!(
            count_active(&mut app),
            3,
            "3 events spaced 3 s apart with a 2.0 s window should produce 3 entities"
        );
    }

    #[test]
    fn test_coalesce_critical_replaces_info() {
        let mut app = build_app(Some(2.0));

        // First event: Info with dedup key "X" at t=0.
        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.0;
        app.world_mut().write_message(event(
            "survey.mission_complete",
            NotificationSeverity::Info,
            Some("X"),
        ));
        app.update();
        assert_eq!(count_active(&mut app), 1);

        // Second event: Critical with the same dedup key at
        // t=0.5 — within the 2.0 s window. The severity upgrade
        // rule means the Critical *replaces* the Info in place
        // (count resets to 1, severity upgrades to Critical).
        // Net: 1 entity, severity = Critical, count = 1.
        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.5;
        app.world_mut().write_message(event(
            "survey.mission_complete",
            NotificationSeverity::Critical,
            Some("X"),
        ));
        app.update();
        assert_eq!(
            count_active(&mut app),
            1,
            "Critical replaces Info in place (severity upgrade)"
        );
        {
            let mut q = app.world_mut().query::<&ActiveNotification>();
            let note = q.iter(app.world()).next().unwrap();
            assert_eq!(note.severity, NotificationSeverity::Critical);
            assert_eq!(note.count, 1, "count resets to 1 on severity upgrade");
        }

        // Third event: a Warning at t=0.6 — within the window.
        // The design rule ("Critical never groups with Info")
        // does not explicitly cover the reverse. Our
        // implementation picks the symmetric behaviour: a
        // higher-rank live entity absorbs lower-rank arrivals
        // (count bumps, severity stays Critical). This keeps
        // the "stuck" Critical visible to the player instead of
        // resetting its timer on every minor event. Documented
        // as a design choice in the PR body.
        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.6;
        app.world_mut().write_message(event(
            "survey.mission_complete",
            NotificationSeverity::Warning,
            Some("X"),
        ));
        app.update();
        assert_eq!(
            count_active(&mut app),
            1,
            "Warning is grouped with the live Critical (no replace)"
        );
        let mut q = app.world_mut().query::<&ActiveNotification>();
        let note = q.iter(app.world()).next().unwrap();
        assert_eq!(note.severity, NotificationSeverity::Critical);
        assert_eq!(note.count, 2, "count bumps even for lower-severity events");
    }

    #[test]
    fn test_coalesce_window_override_flips_boundary() {
        // Same scenario as test 1 (3 events in 1.5 s) but with
        // `default_group_window_s` = 0.5 s. Now the 1.5 s span
        // crosses the boundary and the events should produce 3
        // entities, not 1.
        let mut app = build_app(Some(0.5));

        for t in [0.0_f64, 0.5, 1.0] {
            app.world_mut().resource_mut::<SimulationTime>().elapsed = t;
            app.world_mut().write_message(event(
                "survey.mission_complete",
                NotificationSeverity::Notice,
                Some("mare-imbrium-1"),
            ));
            app.update();
        }

        // The 0.5 s override means Δt = 0.5 from t=0.0 to t=0.5
        // is exactly *at* the boundary. The coalesce condition
        // is `(now - r.created_at) <= group_window`, so the
        // t=0.5 event still matches the t=0.0 entity and bumps
        // count. The t=1.0 event then finds a created_at of
        // 0.5 — Δt = 0.5 = window, so it still matches.
        // Final: 1 entity, count = 3.
        assert_eq!(
            count_active(&mut app),
            1,
            "0.5 s window with 0.5 s gaps still groups (boundary inclusive)"
        );
        let mut q = app.world_mut().query::<&ActiveNotification>();
        let note = q.iter(app.world()).next().unwrap();
        assert_eq!(note.count, 3);

        // Now spawn a fresh app with a 0.1 s window and
        // events at 0.0, 0.5, 1.0 — each Δt > 0.1, so each
        // spawns its own entity. This is the "flips at the
        // new boundary" assertion.
        let mut app2 = build_app(Some(0.1));
        for t in [0.0_f64, 0.5, 1.0] {
            app2.world_mut().resource_mut::<SimulationTime>().elapsed = t;
            app2.world_mut().write_message(event(
                "survey.mission_complete",
                NotificationSeverity::Notice,
                Some("mare-imbrium-1"),
            ));
            app2.update();
        }
        assert_eq!(
            count_active(&mut app2),
            3,
            "0.1 s window with 0.5 s gaps produces 3 entities"
        );
    }

    #[test]
    fn test_coalesce_respects_manifest_disabled_category() {
        // Kilo finding: the manifest's `enabled = false` was
        // silently ignored because the system hard-coded the
        // default to `true`. Build a manifest with a disabled
        // category and assert the event is dropped. Use a
        // category that `settings_with_window` does not pre-seed
        // with a per-category override so the manifest's `enabled`
        // is the only signal `is_category_enabled` sees.
        let mut app = build_app(Some(2.0));
        {
            let mut data = app.world_mut().resource_mut::<NotificationCategoriesData>();
            data.categories.insert(
                "economy.stockpile_critical".into(),
                super::super::super::data::NotificationCategory {
                    id: "economy.stockpile_critical".to_string(),
                    display_name: "Stockpile critical".to_string(),
                    default_dismiss_s: 5.0,
                    enabled: false,
                    pause_on_event: false,
                },
            );
        }

        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.0;
        app.world_mut().write_message(event(
            "economy.stockpile_critical",
            NotificationSeverity::Notice,
            Some("water-low"),
        ));
        app.update();
        assert_eq!(
            count_active(&mut app),
            0,
            "manifest disabled category must be dropped"
        );
    }

    #[test]
    fn test_coalesce_uses_manifest_dismiss_s_not_group_window() {
        // Kilo finding: the auto-dismiss fallback on spawned
        // entities was the global `default_group_window_s`
        // (2.0 s default), not the category's
        // `default_dismiss_s`. Build a manifest with
        // `default_dismiss_s = 7.5` and assert the spawned
        // entity carries 7.5 s, not 2.0 s.
        let mut app = build_app(Some(2.0));
        {
            let mut data = app.world_mut().resource_mut::<NotificationCategoriesData>();
            data.categories.insert(
                "survey.mission_complete".into(),
                crate::ui::notifications::data::NotificationCategory {
                    id: "survey.mission_complete".to_string(),
                    display_name: "Survey complete".to_string(),
                    default_dismiss_s: 7.5,
                    enabled: true,
                    pause_on_event: false,
                },
            );
        }

        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.0;
        app.world_mut().write_message(event(
            "survey.mission_complete",
            NotificationSeverity::Notice,
            // No dedup key — exercises the spawn-new branch.
            None,
        ));
        app.update();
        assert_eq!(count_active(&mut app), 1);
        let mut q = app.world_mut().query::<&ActiveNotification>();
        let note = q.iter(app.world()).next().unwrap();
        assert!(
            (note.auto_dismiss_s - 7.5).abs() < 1e-6,
            "auto-dismiss must inherit manifest default (7.5 s), got {}",
            note.auto_dismiss_s
        );
    }
}
