//! EventBus pub/sub system — the central dispatcher for all game events.
//!
//! Architecture:
//! - `EventBus` is a `World`-local Bevy `Resource`
//! - Game systems subscribe with a callback `Arc<dyn Fn(GameEvent) + Send + Sync>`
//! - When `publish()` is called, all subscribed callbacks are invoked synchronously
//! - Callbacks receive the full `GameEvent` payload and decide what to do (emit notification,
//!   trigger mission, modify state, etc.)
//!
//! Random event timer (per-player):
//! - Checks every 5 min of simulation time
//! - 30% chance to fire a random event from the appropriate pool
//! - 15-min cooldown between random events
//!
//! Alert events fire immediately without a random roll.

use bevy::prelude::*;
use std::sync::Arc;

use crate::events::{EventCategory, EventTag, GameEvent};
use crate::game_events::{EmitNotification, NotificationCategory};
use crate::ui::animations::ToastQueue;

/// A single event subscription.
pub struct EventSubscription {
    callback: Arc<dyn Fn(GameEvent) + Send + Sync>,
    filter_category: Option<EventCategory>,
}

/// Event subscription token — pass to `EventBus::unsubscribe()` to remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    fn to_usize(self) -> usize {
        self.0 as usize
    }
}

// ─── EventBus resource ─────────────────────────────────────────────────────────

/// Central pub/sub event dispatcher — a Bevy `Resource` local to the `World`.
///
/// Systems subscribe via `event_bus.subscribe()` and receive call back on `publish()`.
#[derive(Resource)]
pub struct EventBus {
    subscriptions: Vec<EventSubscription>,
    active: Vec<bool>,
    next_id: u64,
}

impl Default for EventBus {
    fn default() -> Self {
        Self {
            subscriptions: Vec::new(),
            active: Vec::new(),
            next_id: 0,
        }
    }
}

impl EventBus {
    /// Subscribe to events matching `category`. If `category` is `None`, receives all events.
    ///
    /// Returns a `SubscriptionId` token for later `unsubscribe()`.
    pub fn subscribe(
        &mut self,
        category: Option<EventCategory>,
        callback: impl Fn(GameEvent) + Send + Sync + 'static,
    ) -> SubscriptionId {
        let id = SubscriptionId(self.next_id);
        self.next_id += 1;
        self.subscriptions.push(EventSubscription {
            callback: Arc::new(callback),
            filter_category: category,
        });
        self.active.push(true);
        id
    }

    /// Remove a subscription by its token. No-op if the token is not found or already inactive.
    pub fn unsubscribe(&mut self, id: SubscriptionId) {
        let idx = id.to_usize();
        if idx < self.active.len() {
            self.active[idx] = false;
        }
    }

    /// Publish an event to all matching subscribers.
    /// Callbacks are invoked synchronously in subscription order, skipping inactive slots.
    pub fn publish(&self, event: GameEvent) {
        for (i, sub) in self.subscriptions.iter().enumerate() {
            if !self.active[i] {
                continue;
            }
            let matches = match (&sub.filter_category, &event) {
                (None, _) => true,
                (Some(EventCategory::Alert), GameEvent::Alert { .. }) => true,
                (Some(EventCategory::Random), GameEvent::Random { .. }) => true,
                (Some(EventCategory::Story), GameEvent::Story { .. }) => true,
                _ => false,
            };
            if matches {
                (sub.callback)(event.clone());
            }
        }
    }

    /// Returns the number of active subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.active.iter().filter(|&&a| a).count()
    }
}

// ─── Random event timer resource ──────────────────────────────────────────────

/// Per-player random event timer state.
#[derive(Resource)]
pub struct RandomEventTimer {
    /// Simulation seconds elapsed since last random event fired.
    pub elapsed_since_last_event: f64,
    /// Simulation time (in seconds) when the last event fired.
    pub last_event_time: f64,
}

impl Default for RandomEventTimer {
    fn default() -> Self {
        Self {
            elapsed_since_last_event: f64::MAX, // fire immediately on first check
            last_event_time: 0.0,
        }
    }
}

impl RandomEventTimer {
    /// Check interval in simulation seconds (5 minutes).
    pub const CHECK_INTERVAL_SECS: f64 = 5.0 * 60.0;
    /// Cooldown between random events in simulation seconds (15 minutes).
    pub const COOLDOWN_SECS: f64 = 15.0 * 60.0;
    /// Probability of firing when check elapses.
    pub const ROLL_CHANCE: f64 = 0.30;

    /// Call after a random event fires to reset the cooldown.
    pub fn on_event_fired(&mut self, current_sim_time: f64) {
        self.last_event_time = current_sim_time;
        self.elapsed_since_last_event = 0.0;
    }

    /// Advance the timer by `delta_secs` simulation seconds.
    pub fn update(&mut self, delta_secs: f64) {
        self.elapsed_since_last_event += delta_secs;
    }

    /// Simulation seconds since last event.
    pub fn elapsed(&self) -> f64 {
        self.elapsed_since_last_event
    }

    /// Simulation time when the last event fired.
    pub fn last_event_time(&self) -> f64 {
        self.last_event_time
    }

    /// Whether the cooldown period has elapsed (allows next check to proceed to roll).
    pub fn cooldown_elapsed(&self) -> bool {
        self.elapsed_since_last_event >= Self::COOLDOWN_SECS
    }
}

// ─── Notification helpers ─────────────────────────────────────────────────────

/// Convert a `GameEvent` to an `EmitNotification` for routing to the notification system.
pub fn game_event_to_notification(event: &GameEvent) -> EmitNotification {
    match event {
        GameEvent::Alert {
            title,
            description,
            tags,
            ..
        } => {
            let category = category_from_tags(tags);
            EmitNotification::critical(title.clone(), description.clone())
                .with_category(category)
        }
        GameEvent::Random {
            title,
            description,
            tags,
            ..
        } => {
            let category = category_from_tags(tags);
            EmitNotification::warning(title.clone(), description.clone())
                .with_category(category)
        }
        GameEvent::Story {
            title,
            description,
            ..
        } => EmitNotification::critical(title.clone(), description.clone()),
        GameEvent::Tags(_) => EmitNotification::info("Event", "An event occurred"),
    }
}

pub fn category_from_tags(tags: &[EventTag]) -> NotificationCategory {
    for tag in tags {
        match tag {
            EventTag::Combat => return NotificationCategory::Combat,
            EventTag::Diplomacy => return NotificationCategory::Diplomacy,
            EventTag::Economy => return NotificationCategory::Resource,
            EventTag::Research => return NotificationCategory::Research,
            _ => {}
        }
    }
    NotificationCategory::System
}

// ─── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that registers the `EventBus` and `RandomEventTimer` resources.
pub struct EventBusPlugin;

impl Plugin for EventBusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventBus>()
            .init_resource::<RandomEventTimer>();
    }
}