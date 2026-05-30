//! EventBus systems — random event timing and alert firing.
//!
//! Random event timer: checks every 5 simulation minutes, 30% roll chance,
//! 15-min cooldown between random events. Alert events fire synchronously.

use rand::Rng;

use bevy::prelude::*;

use crate::events::bus::{EventBus, RandomEventTimer};
use crate::events::load_events::EventsData;
use crate::game_events::{EmitNotification, NotificationCategory};
use crate::ui::animations::{ToastKind, ToastMessage, ToastQueue};
use crate::ui::time::SimulationTime;

const CHECK_INTERVAL_SECS: f64 = 5.0 * 60.0; // 5 simulation minutes
const COOLDOWN_SECS: f64 = 15.0 * 60.0; // 15 simulation minutes
const ROLL_CHANCE: f64 = 0.30;

/// Toast duration for important random events (e.g. disaster pool) — 8 seconds.
/// Auto-dismiss after this time.
const TOAST_DURATION_IMPORTANT_SECS: f32 = 8.0;

/// Toast duration for critical events (story milestone, alert disaster).
/// Set to a very long duration to approximate "manual dismiss" behavior
/// until a dedicated dismiss flag is implemented.
const TOAST_DURATION_CRITICAL_SECS: f32 = 3600.0;

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
    mut event_bus: ResMut<EventBus>,
    mut notification_events: EventWriter<EmitNotification>,
    mut toast_queue: ResMut<ToastQueue>,
) {
    let elapsed = sim_time.elapsed_seconds();

    // Compute the current check window (floor to 5-min boundary)
    let current_window = (elapsed / CHECK_INTERVAL_SECS).floor() as u64;
    let last_window = (timer.last_event_time / CHECK_INTERVAL_SECS).floor() as u64;

    // Only process once per check window
    if current_window <= last_window {
        return;
    }

    // Cooldown: has at least 15 sim-minutes passed since last event?
    let time_since_last_event = elapsed - timer.last_event_time;
    if time_since_last_event < COOLDOWN_SECS {
        return;
    }

    // Roll 30% — if failed, do nothing this check window
    if rand::thread_rng().gen::<f64>() >= ROLL_CHANCE {
        // Don't update last_event_time on a missed roll — try again next window
        return;
    }

    // Select a random pool (discovery, disaster, opportunity)
    let pool_idx = rand::thread_rng().gen_range(0..3);
    let pool = match pool_idx {
        0 => &events_data.discovery_pool,
        1 => &events_data.disaster_pool,
        2 => &events_data.opportunity_pool,
        _ => return,
    };

    if pool.is_empty() {
        return;
    }

    let event = pool[rand::thread_rng().gen_range(0..pool.len())].clone();

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
        .with_category(categorize_tags(&event.tags));
    notification_events.send(notification);

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

/// System: fire an alert (crisis) event immediately — no random roll, no cooldown.
///
/// Game systems call this directly when a crisis condition is met
/// (e.g., enemy fleet detected, colony stability collapse). The event
/// publishes to the bus and routes to the notification system.
///
/// Per DELA-14 spec: Alert disaster → Critical (full toast + banner + alert sound).
/// Uses ToastKind::Error with a very long duration to approximate manual dismiss.
pub fn fire_alert_event(
    event_bus: Res<EventBus>,
    event_id: super::EventId,
    title: String,
    description: String,
    tags: Vec<super::EventTag>,
    mut notification_events: EventWriter<EmitNotification>,
    mut toast_queue: ResMut<ToastQueue>,
) {
    let game_event = super::GameEvent::Alert {
        event_id,
        title: title.clone(),
        description: description.clone(),
        tags: tags.clone(),
    };

    event_bus.publish(game_event);

    let notification = EmitNotification::critical(&title, &description)
        .with_category(categorize_tags(&tags));
    notification_events.send(notification);

    let mut toast = ToastMessage::new(format!("ALERT: {}", title), ToastKind::Error);
    toast.duration_secs = TOAST_DURATION_CRITICAL_SECS;
    toast_queue.push(toast);
}

/// System: fire a story milestone event.
///
/// Story milestones are triggered by game state (act progression, major discoveries).
/// Per DELA-14 spec: Story milestone → Critical (full toast + banner, manual dismiss).
///
/// This function should be called by game systems when a story milestone condition
/// is met (e.g., first colony established, campaign act transition).
pub fn fire_story_event(
    event_bus: Res<EventBus>,
    event_id: super::EventId,
    title: String,
    description: String,
    mut notification_events: EventWriter<EmitNotification>,
    mut toast_queue: ResMut<ToastQueue>,
) {
    let game_event = super::GameEvent::Story {
        event_id,
        title: title.clone(),
        description: description.clone(),
    };

    event_bus.publish(game_event);

    // Story milestones are critical — use critical notification with sound
    let notification = EmitNotification::critical(title.clone(), description.clone());
    notification_events.send(notification);

    // Critical toast: long duration to approximate manual dismiss behavior
    let mut toast = ToastMessage::new(
        format!("Story Milestone: {}", title),
        ToastKind::Error,
    );
    toast.duration_secs = TOAST_DURATION_CRITICAL_SECS;
    toast_queue.push(toast);
}

fn categorize_tags(tags: &[super::EventTag]) -> NotificationCategory {
    use crate::game_events::NotificationCategory;
    for tag in tags {
        match tag {
            super::EventTag::Combat => return NotificationCategory::Combat,
            super::EventTag::Diplomacy => return NotificationCategory::Diplomacy,
            super::EventTag::Economy => return NotificationCategory::Resource,
            super::EventTag::Research => return NotificationCategory::Research,
            _ => {}
        }
    }
    NotificationCategory::System
}