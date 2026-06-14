//! End-to-end integration tests for the notifications feature
//! (Helios Ascension PR-H, GRA-142).
//!
//! These tests exercise the full `NotificationEvent` → `coalesce` →
//! `ActiveNotification` → `auto_dismiss_toasts` → entity-despawned
//! pipeline and the cross-cutting render / settings / RON /
//! pause-on-event surfaces.
//!
//! # Bevy 0.18 message-buffer trap
//!
//! Bevy 0.18's `Messages<T>` is **double-buffered**: a
//! `MessageReader` only sees events written by `MessageWriter` after
//! a per-frame `message_update_system` swap. `App::update()` runs
//! that system implicitly, but a bare `Schedule::run(&mut world)`
//! does **not** — readers see an empty buffer. The first attempt at
//! this file used raw `Schedule` and 3 of 5 runnable tests failed
//! silently (the bridge produced nothing, coalesce saw no events).
//! This file uses `App::new()` + `app.update()` exclusively, which
//! is the same pattern the existing unit tests in
//! `src/ui/notifications/systems/coalesce.rs` and
//! `.../event_bridge.rs` use, and which automatically wires the
//! message-update system.
//!
//! # Stacked-PR scope
//!
//! PR-H is stacked on PR-D (GRA-138). PR-C (GRA-137) and PR-F
//! (GRA-140) merged to main the same day PR-H was resumed; both
//! `event_bridge` and `pause_on_event_toasts` are available on the
//! base. Three tests are `#[ignore]`-d:
//!
//! * #5 (render `show_only_in_survey` early-return) — render system
//!   needs the egui harness, drops for the same reason as the
//!   GRA-139 unit tests (see [[feedback-egui-render-tests]]).
//! * #6 (render `max_visible_toasts` truncation) — same harness
//!   dependency.
//! * #8 (RON severity clamp) — spec delta: `NotificationCategory`
//!   has no `severity` field and `NotificationSeverity` has no
//!   `Deserialize` impl. Re-enable after a PR-A extension.
//!
//! Run `cargo test --test notifications_e2e -- --ignored` to
//! verify the three ignored tests once their dependencies land.

use bevy::prelude::*;
use helios_ascension::game_state::{ActiveMenu, GameMenu};
use helios_ascension::survey::events::SurveyEvent;
use helios_ascension::ui::notifications::components::{
    ActiveNotification, PendingNotificationDismissal,
};
use helios_ascension::ui::notifications::data::{NotificationCategoriesData, NotificationCategory};
use helios_ascension::ui::notifications::events::{NotificationEvent, NotificationSeverity};
use helios_ascension::ui::notifications::settings::{
    NotificationCategoryId, NotificationSettings, PerCategorySetting,
};
use helios_ascension::ui::notifications::systems::coalesce::coalesce_notifications;
use helios_ascension::ui::notifications::systems::event_bridge::bridge_survey_events;
use helios_ascension::ui::notifications::systems::tick::{
    apply_pending_dismissals, auto_dismiss_toasts, pause_on_event_toasts,
};
use helios_ascension::ui::time::{SimulationTime, TimeScale};

/// Build a Bevy `App` with the resources, message buses, and
/// coalesce/tick systems the notifications feature needs. The
/// caller adds the bridge systems or extra `app.update()` calls.
///
/// `App::new()` (not `World::new()` + `Schedule`) is required for
/// the Bevy 0.18 message double-buffer to flush — see the module
/// header for the trap.
///
/// All resources / message buses the schedule's systems touch are
/// registered up front so a per-test `app.update()` is enough —
/// the schedule runs every system every tick, and Bevy 0.18
/// panics on a missing `ResMut<T>` or uninitialised `Messages<T>`.
/// The bridge system (`bridge_survey_events`) reads
/// `Messages<SurveyEvent>` even when no SurveyEvent is being
/// tested (Bevy validates system params before the schedule body
/// runs), so we register the source bus unconditionally.
fn build_app() -> App {
    let mut app = App::new();
    app.init_resource::<SimulationTime>();
    app.init_resource::<NotificationSettings>();
    app.init_resource::<NotificationCategoriesData>();
    app.init_resource::<PendingNotificationDismissal>();
    app.init_resource::<ActiveMenu>();
    app.init_resource::<TimeScale>();
    // The owning plugin does not call `app.add_message::<NotificationEvent>()`
    // today (a known production-side gap — see PR-H body); the test
    // harness registers it explicitly so the coalesce system doesn't
    // panic with "Message not initialized" when the bridge writes
    // one. Also register the source-event buses the bridges need.
    app.add_message::<NotificationEvent>();
    app.add_message::<SurveyEvent>();
    app.add_systems(
        Update,
        (
            bridge_survey_events,
            coalesce_notifications,
            auto_dismiss_toasts,
            apply_pending_dismissals,
            pause_on_event_toasts,
        ),
    );
    app
}

/// Spawn a `NotificationEvent` into the message bus. The
/// `coalesce_notifications` system picks it up on the next
/// `app.update()` tick and creates an `ActiveNotification` entity.
fn fire_event(app: &mut App, event: NotificationEvent) {
    app.world_mut().write_message(event);
}

fn count_active(app: &mut App) -> usize {
    let mut q = app.world_mut().query::<&ActiveNotification>();
    q.iter(app.world()).count()
}

// ── Test 1 ───────────────────────────────────────────────────────
// SurveyEvent → bridge → coalesce → tick → entity despawned.
// End-to-end: a survey mission completion event rides the PR-C
// bridge into a NotificationEvent, coalesce spawns an
// ActiveNotification with a small auto-dismiss_s, then the
// dismiss-tick system despawns it when SimulationTime advances.
#[test]
fn test_full_event_to_dismissed_toast_lifecycle() {
    use helios_ascension::survey::types::SurveyMethod;

    let mut app = build_app();
    // The bridge uses `Query<&Name>` for body-name text. The
    // `Messages<SurveyEvent>` bus is registered in `build_app` so
    // the schedule can run for every test (Bevy validates system
    // params before the schedule body).
    let body_entity = app.world_mut().spawn(Name::new("Mars Test Body")).id();

    // Override the spawned toast's auto-dismiss to 0.1 s so the
    // test runs in microseconds. The category's manifest default
    // is 6.0 s. We can't tweak it pre-spawn because the spawn path
    // copies `event.auto_dismiss_s.unwrap_or(manifest_default)`;
    // pass `Some(0.1)` in the event instead. The bridge sets this
    // from the source event's payload (`auto_dismiss_s` is not on
    // `SurveyEvent::MissionCompleted` — the bridge falls back to
    // `None`, which means the coalesce system will use the
    // manifest default). To force a short dismiss, mutate the
    // entity directly post-spawn (the same hack the original
    // `Schedule` test used).
    app.world_mut()
        .write_message(SurveyEvent::MissionCompleted {
            body: body_entity,
            mission_id: 1,
            name: "Test Survey".to_string(),
            method: SurveyMethod::Orbital,
        });
    app.update();

    // Bridge + coalesce produced 1 entity.
    assert_eq!(
        count_active(&mut app),
        1,
        "bridge + coalesce must produce exactly 1 ActiveNotification for 1 SurveyEvent::MissionCompleted"
    );

    // Force a short auto-dismiss so the test doesn't need 6 s of
    // simulated time. The dismiss-timer logic in
    // `auto_dismiss_toasts` uses `now - created_at` against this
    // field.
    {
        let mut q = app.world_mut().query::<&mut ActiveNotification>();
        for mut note in q.iter_mut(app.world_mut()) {
            note.auto_dismiss_s = 0.1;
        }
    }

    // Advance the simulation clock past the dismiss window.
    app.world_mut().resource_mut::<SimulationTime>().elapsed = 1_000.0;
    app.update();

    assert_eq!(
        count_active(&mut app),
        0,
        "auto-dismiss tick must despawn the entity after the timer elapses"
    );
}

// ── Test 2 ───────────────────────────────────────────────────────
// Per-category `enabled = false` override drops the event in
// coalesce. Asserts no `ActiveNotification` entity was created.
#[test]
fn test_settings_override_disables_category() {
    let mut app = build_app();
    {
        let mut settings = app.world_mut().resource_mut::<NotificationSettings>();
        settings.per_category.insert(
            NotificationCategoryId::from("survey.mission_complete"),
            PerCategorySetting {
                enabled: false,
                pause_on_event: false,
                sound_on: false,
                auto_dismiss_s: 0.0,
                sticky: false,
            },
        );
    }

    fire_event(
        &mut app,
        NotificationEvent {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Info,
            title: "should be dropped".to_string(),
            body: String::new(),
            dedup_key: Some("mission:1".to_string()),
            auto_dismiss_s: Some(10.0),
            sticky: false,
        },
    );
    app.update();

    assert_eq!(
        count_active(&mut app),
        0,
        "per-category enabled=false must drop the event before spawning an entity"
    );
}

// ── Test 3 ───────────────────────────────────────────────────────
// `default_group_window_s` is 2.0 s by default. With the default
// setting, two events with the same dedup_key inside the window
// coalesce into one entity with `count == 2`. Overriding the
// window to 0.5 s means two events 1.0 s apart do NOT coalesce;
// with a 1.5 s override the same 1.0 s gap DOES coalesce. The
// coalesce condition is `(now - r.created_at) <= group_window`,
// so the boundary is inclusive.
#[test]
fn test_group_window_default_then_overridden() {
    // ── Default 2.0 s window: two events at t=0 and t=1.0 ──
    let mut app = build_app();
    app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.0;

    for i in 0..2 {
        fire_event(
            &mut app,
            NotificationEvent {
                category: NotificationCategoryId::from("survey.mission_complete"),
                severity: NotificationSeverity::Info,
                title: format!("repeat {i}"),
                body: String::new(),
                dedup_key: Some("mission:42".to_string()),
                auto_dismiss_s: Some(60.0),
                sticky: false,
            },
        );
    }
    app.update();

    {
        let mut q = app.world_mut().query::<&ActiveNotification>();
        let notes: Vec<&ActiveNotification> = q.iter(app.world()).collect();
        assert_eq!(
            notes.len(),
            1,
            "two same-dedup_key events in the default 2.0 s window must coalesce into 1 entity; got {}",
            notes.len()
        );
        assert_eq!(
            notes[0].count, 2,
            "coalesced entity must have count=2 after 2 same-dedup_key events"
        );
    }

    // ── Override 0.5 s: events 1.0 s apart do NOT coalesce ──
    let mut app = build_app();
    {
        let mut t = app.world_mut().resource_mut::<SimulationTime>();
        t.elapsed = 0.0;
        let mut settings = app.world_mut().resource_mut::<NotificationSettings>();
        settings.default_group_window_s = 0.5;
    }

    fire_event(
        &mut app,
        NotificationEvent {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Info,
            title: "first".to_string(),
            body: String::new(),
            dedup_key: Some("mission:99".to_string()),
            auto_dismiss_s: Some(60.0),
            sticky: false,
        },
    );
    app.update();
    app.world_mut().resource_mut::<SimulationTime>().elapsed = 1.0;
    fire_event(
        &mut app,
        NotificationEvent {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Info,
            title: "second".to_string(),
            body: String::new(),
            dedup_key: Some("mission:99".to_string()),
            auto_dismiss_s: Some(60.0),
            sticky: false,
        },
    );
    app.update();

    assert_eq!(
        count_active(&mut app),
        2,
        "with default_group_window_s=0.5, two same-dedup_key events 1.0 s apart must NOT coalesce"
    );

    // ── Sanity: the override really is honored. With a 1.5 s
    // override the SAME 1.0 s gap WOULD coalesce. ──
    let mut app = build_app();
    {
        let mut t = app.world_mut().resource_mut::<SimulationTime>();
        t.elapsed = 0.0;
        let mut settings = app.world_mut().resource_mut::<NotificationSettings>();
        settings.default_group_window_s = 1.5;
    }
    fire_event(
        &mut app,
        NotificationEvent {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Info,
            title: "first".to_string(),
            body: String::new(),
            dedup_key: Some("mission:99".to_string()),
            auto_dismiss_s: Some(60.0),
            sticky: false,
        },
    );
    app.update();
    app.world_mut().resource_mut::<SimulationTime>().elapsed = 1.0;
    fire_event(
        &mut app,
        NotificationEvent {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Info,
            title: "second".to_string(),
            body: String::new(),
            dedup_key: Some("mission:99".to_string()),
            auto_dismiss_s: Some(60.0),
            sticky: false,
        },
    );
    app.update();

    {
        let mut q = app.world_mut().query::<&ActiveNotification>();
        let notes: Vec<&ActiveNotification> = q.iter(app.world()).collect();
        assert_eq!(
            notes.len(),
            1,
            "with default_group_window_s=1.5, two same-dedup_key events 1.0 s apart MUST coalesce; got {}",
            notes.len()
        );
        assert_eq!(
            notes[0].count, 2,
            "coalesced entity must have count=2 after 2 same-dedup_key events within the 1.5 s window"
        );
    }
}

// ── Test 4 ───────────────────────────────────────────────────────
// `pause_on_event=true` category → toast inserted →
// TimeScale::pause() called. The `pause_on_event_toasts` system
// uses a `Local<HashSet<Entity>>` cache to detect newly-inserted
// entities — we need at least two `app.update()` calls (one to
// insert + prime the cache, one to detect + pause), OR we can
// pre-populate the cache via a no-op update.
#[test]
fn test_pause_on_event_chain() {
    let mut app = build_app();
    // Populate the categories manifest with a row that requests
    // pause_on_event. The startup loader is not invoked here (we
    // use a minimal App); we set the manifest directly.
    {
        let mut cats = app.world_mut().resource_mut::<NotificationCategoriesData>();
        cats.categories.insert(
            NotificationCategoryId::from("survey.mission_complete"),
            NotificationCategory {
                id: "survey.mission_complete".to_string(),
                display_name: "Mission complete".to_string(),
                default_dismiss_s: 30.0,
                enabled: true,
                pause_on_event: true,
            },
        );
    }

    // First update with no events primes the
    // `Local<HashSet<Entity>>` cache in `pause_on_event_toasts` —
    // the cache is empty on construction, so without this the
    // first toast would always be "new" regardless of when it
    // was inserted.
    app.update();

    // Fire the event. Coalesce will spawn the entity; the
    // `pause_on_event_toasts` system notices the new entity
    // and pauses TimeScale.
    fire_event(
        &mut app,
        NotificationEvent {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Critical,
            title: "Survey done".to_string(),
            body: String::new(),
            dedup_key: None,
            auto_dismiss_s: Some(30.0),
            sticky: false,
        },
    );
    app.update();

    // TimeScale must be paused.
    let time_scale = app.world().resource::<TimeScale>();
    assert!(
        time_scale.is_paused(),
        "TimeScale must be paused after a pause_on_event=true toast was inserted"
    );
}

// ── Test 5 ───────────────────────────────────────────────────────
// `show_only_in_survey = true` (the default) and
// `ActiveMenu.current = Construction` → render system early-outs
// before doing any work.
//
// The render system touches `bevy_egui::EguiContexts`, which is
// not available in a minimal `App` — the system would panic on
// `contexts.ctx_mut()` if it got that far. The contract is
// therefore observable only via the early-return guard
// (`render.rs:61-65`): the system must return without touching
// egui when the active menu is non-Survey. The harness here
// therefore does NOT insert an egui context; if the early-return
// regressed, the system would panic with "EguiContexts not in
// world" rather than running cleanly. That panic is the
// regression signal.
//
// Same harness-deficiency as the GRA-139 settings panel tests —
// `cargo test` cannot drive the egui render path; a separate
// integration test that uses the egui harness is the only path
// to end-to-end coverage.
#[test]
#[ignore = "render system needs the bevy_egui::EguiContexts resource installed, which requires the egui harness; the system panics at parameter validation before reaching the show_only_in_survey guard. The early-return guard itself is a one-line predicate and is covered by code review. See PR-H body for the egui-harness follow-up."]
fn test_render_runs_in_survey_only_by_default() {
    use helios_ascension::ui::notifications::systems::render::render_notification_toasts;

    let mut app = build_app();
    // Default `show_only_in_survey` is true and `ActiveMenu`
    // defaults to `GameMenu::Survey`, so the default path
    // wouldn't even hit the guard. Force the guard by switching
    // the menu to `Construction`.
    app.world_mut().resource_mut::<ActiveMenu>().current = GameMenu::Construction;

    // Spawn a toast so the render system has work to skip.
    app.world_mut().spawn(ActiveNotification {
        category: NotificationCategoryId::from("survey.mission_complete"),
        severity: NotificationSeverity::Info,
        title: "should not render on Construction".to_string(),
        body: String::new(),
        created_at: 0.0,
        auto_dismiss_s: 30.0,
        sticky: false,
        dedup_key: None,
        count: 1,
    });
    app.add_systems(Update, render_notification_toasts);

    // No `bevy_egui::EguiContexts` is installed. The render
    // system must not panic — its `show_only_in_survey` guard
    // returns before `contexts.ctx_mut()`.
    app.update();

    // The entity must still be alive (the render system does
    // not despawn).
    assert_eq!(count_active(&mut app), 1);
}

// ── Test 6 ───────────────────────────────────────────────────────
// With `max_visible_toasts = 5` and 10 spawned
// `ActiveNotification` entities, the render system's truncation
// logic caps the rendered list at 5. Same harness dependency as
// test #5 — the render system reaches `contexts.ctx_mut()` only
// after the truncation step, and `bevy_egui::EguiContexts` is
// not installed in the harness — so we assert only the
// observable part of the system: the entities are not despawned
// by the render path.
#[test]
#[ignore = "render system truncation requires the egui harness; the test runs cleanly with 0 toasts but the truncate step is not observable without bevy_egui::EguiContexts. See PR-H body for the egui-harness follow-up."]
fn test_max_visible_toasts_caps_render() {
    use helios_ascension::ui::notifications::systems::render::render_notification_toasts;

    let mut app = build_app();
    {
        let mut settings = app.world_mut().resource_mut::<NotificationSettings>();
        settings.max_visible_toasts = 5;
        settings.show_only_in_survey = false;
    }
    // Spawn 10 toasts.
    for i in 0..10 {
        app.world_mut().spawn(ActiveNotification {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Info,
            title: format!("toast {i}"),
            body: String::new(),
            created_at: i as f64,
            auto_dismiss_s: 30.0,
            sticky: false,
            dedup_key: None,
            count: 1,
        });
    }
    app.add_systems(Update, render_notification_toasts);
    app.update();

    assert_eq!(
        count_active(&mut app),
        10,
        "render system must not despawn entities"
    );
}

// ── Test 7 ───────────────────────────────────────────────────────
// `sticky=true` toast survives the auto-dismiss timer.
#[test]
fn test_sticky_toast_never_dismisses_on_timer() {
    let mut app = build_app();
    app.world_mut().resource_mut::<SimulationTime>().elapsed = 1_000_000.0;

    let id = app
        .world_mut()
        .spawn(ActiveNotification {
            category: NotificationCategoryId::from("survey.mission_complete"),
            severity: NotificationSeverity::Critical,
            title: "Do not dismiss".to_string(),
            body: String::new(),
            created_at: 0.0,
            auto_dismiss_s: 0.1,
            sticky: true,
            dedup_key: None,
            count: 1,
        })
        .id();

    // The auto-dismiss timer has long expired (1_000_000 s > 0.1 s)
    // but `sticky=true` must skip the timer.
    app.update();

    assert!(
        app.world().get_entity(id).is_ok(),
        "sticky toast must survive the auto-dismiss timer"
    );
}

// ── Test 8 ───────────────────────────────────────────────────────
// `assets/data/notifications.ron` with a row whose `severity`
// field is `"Bogus"` deserializes to `Info` with a warning.
//
// Spec delta (called out in the GRA-142 blocked-comment, 2026-06-14
// ~11:27 Z):
//   - `NotificationCategory` does NOT have a `severity` field
//     today (the RON schema is id / display_name /
//     default_dismiss_s / enabled / pause_on_event).
//   - `NotificationSeverity` does NOT derive `Deserialize`.
// The "clamp to Info on unknown severity" behaviour therefore
// cannot be expressed against the current loader. Re-enable
// after PR-A is extended with a `severity` field on
// `NotificationCategory` AND a permissive `Deserialize` impl on
// `NotificationSeverity` (one of:
//   * `#[serde(other)]` on an `Unknown` variant
//   * a custom Visitor that maps unknowns to `Info`
// ).
#[test]
#[ignore = "spec delta: NotificationCategory has no `severity` field and NotificationSeverity has no Deserialize impl. Requires a PR-A extension (severity field on the manifest + permissive Deserialize on the enum). Re-enable when that lands."]
fn test_ron_loader_clamps_unknown_severity() {
    // Intentionally empty. The full implementation will:
    //   1. write a temp RON file with a category whose
    //      `severity: "Bogus"`
    //   2. call `load_notification_categories` against it
    //   3. assert the loaded category's severity is `Info`
    //   4. assert a `warn!` was emitted (capture via
    //      `tracing-test` or a custom `tracing_subscriber`
    //      layer — PR-H does not introduce that harness today)
    let _app = build_app();
}
