//! EventBus systems — random event timing and alert firing.
//!
//! Random event timer: checks every 5 simulation minutes, 30% roll chance,
//! 15-min cooldown between random events. Alert events fire synchronously.

use std::cell::RefCell;
use std::time::Instant;

use bevy::prelude::*;
use rand;

use crate::events::{category_from_tags, EventBus, EventsData, RandomEventTimer};
use crate::game_events::{EmitNotification, Notification, NotificationQueue};
use crate::ui::animations::{ToastKind, ToastMessage, ToastQueue};
use crate::ui::time::SimulationTime;

/// Stores the previous simulation elapsed for delta computation.
thread_local! {
    static PREV_ELAPSED: RefCell<f64> = RefCell::new(0.0);
}

/// System: advance the random event timer each frame.
pub fn advance_random_timer(
    mut timer: ResMut<RandomEventTimer>,
    sim_time: Res<SimulationTime>,
) {
    let current = sim_time.elapsed_seconds();
    let prev = PREV_ELAPSED.with(|cell| {
        let mut prev = cell.borrow_mut();
        let delta = current - *prev;
        *prev = current;
        delta
    });
    timer.update(prev);
}

/// Toast duration for important random events (e.g. disaster pool) — 8 seconds.
/// Auto-dismiss after this time.
const TOAST_DURATION_IMPORTANT_SECS: f32 = 8.0;

/// Toast duration for critical events (story milestone, alert disaster).
/// Set to a very long duration to approximate "manual dismiss" behavior.
pub const TOAST_DURATION_CRITICAL_SECS: f32 = 3600.0;

/// System: check for random event trigger every 5 simulation minutes.
///
/// On each check window that passes the cooldown:
/// 1. Roll 30% — if failed, do nothing
/// 2. Pick a random pool (discovery/disaster/opportunity)
/// 3. Pick a random event from that pool
/// 4. Publish `GameEvent::Random` to the bus
/// 5. Send `EmitNotification` to the notification system
/// 6. Push a toast to the toast queue
///
/// Cooldown: 15 simulation minutes between random events.
/// Alert events (EventCategory::Alert) fire immediately via `fire_alert_event()`,
/// bypassing this timer entirely.
pub fn check_random_event_timing(
    mut timer: ResMut<RandomEventTimer>,
    sim_time: Res<SimulationTime>,
    events_data: Res<EventsData>,
    event_bus: ResMut<EventBus>,
    mut notification_queue: ResMut<NotificationQueue>,
    mut toast_queue: ResMut<ToastQueue>,
) {
    let elapsed = sim_time.elapsed_seconds();

    // Compute the current check window (floor to 5-min boundary)
    let current_window = (elapsed / RandomEventTimer::CHECK_INTERVAL_SECS).floor() as u64;
    let last_window = (timer.last_event_time() / RandomEventTimer::CHECK_INTERVAL_SECS).floor() as u64;

    // Only process once per check window
    if current_window <= last_window {
        return;
    }

    // Cooldown: has at least 15 sim-minutes passed since last event?
    let time_since_last_event = elapsed - timer.last_event_time();
    if time_since_last_event < RandomEventTimer::COOLDOWN_SECS {
        return;
    }

    // Roll 30% — if failed, do nothing this check window
    if rand::random::<f64>() >= RandomEventTimer::ROLL_CHANCE {
        // Don't update last_event_time on a missed roll — try again next window
        return;
    }

    // Select a random pool (discovery, disaster, opportunity)
    let pool_idx = rand::random_range(0..3);
    let pool = match pool_idx {
        0 => &events_data.discovery_pool,
        1 => &events_data.disaster_pool,
        2 => &events_data.opportunity_pool,
        _ => return,
    };

    if pool.is_empty() {
        return;
    }

    let event = pool[rand::random_range(0..pool.len())].clone();

    // Build GameEvent payload
    let game_event = super::GameEvent::Random {
        event_id: event.id,
        title: event.title.clone(),
        description: event.description.clone(),
        tags: event.tags.clone(),
    };

    // Publish to EventBus — subscribers decide how to react
    event_bus.publish(game_event);

    // Route to notification system
    let notification = EmitNotification::warning(&event.title, &event.description)
        .with_category(category_from_tags(&event.tags));
    let notification = Notification {
        id: 0, // assigned by queue.push
        kind: notification.kind,
        category: notification.category,
        title: notification.title,
        body: notification.body,
        entity: notification.entity,
        arrived_at: Instant::now(),
        acknowledged: false,
    };
    notification_queue.push(notification);

    // Map pool to notification tier per DELA-14 spec:
    // - Discovery/Opportunity → Casual (no toast, scrolling feed only)
    // - Disaster → Important (toast, 8 seconds)
    let toast_kind = match pool_idx {
        0 => None,                           // Discovery → Casual (no toast)
        1 => Some(ToastKind::Warning),        // Disaster → Important (Warning toast)
        2 => None,                           // Opportunity → Casual (no toast)
        _ => None,
    };

    if let Some(kind) = toast_kind {
        let mut toast = ToastMessage::new(
            format!("Random Event: {}", event.title),
            kind,
        );
        toast.duration_secs = TOAST_DURATION_IMPORTANT_SECS;
        toast_queue.push(toast);
    }

    // Record event time to restart cooldown and avoid double-firing this window
    timer.last_event_time = elapsed;
    timer.elapsed_since_last_event = 0.0;
}

/// Fire an alert (crisis) event immediately — no random roll, no cooldown.
/// Game systems call this directly when a crisis condition is met.
/// Per DELA-14 spec: Alert disaster → Critical (full toast + banner + alert sound).
/// Uses ToastKind::Error with a very long duration to approximate manual dismiss.
pub fn fire_alert_event(
    event_bus: &EventBus,
    event_id: &str,
    title: &str,
    description: &str,
    tags: &[super::EventTag],
) -> GameEvent {
    let event = super::GameEvent::Alert {
        event_id: event_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        tags: tags.to_vec(),
    };
    event_bus.publish(event.clone());
    event
}

/// Fire a story milestone event.
/// Story milestones are triggered by game state (act progression, major discoveries).
/// Per DELA-14 spec: Story milestone → Critical (full toast + banner, manual dismiss).
pub fn fire_story_event(
    event_bus: &EventBus,
    event_id: &str,
    title: &str,
    description: &str,
) -> GameEvent {
    let event = super::GameEvent::Story {
        event_id: event_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
    };
    event_bus.publish(event.clone());
    event
}