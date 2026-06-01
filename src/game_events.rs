//! Game event and notification system.
//!
//! Emits typed events from game systems and manages a queue of on-screen
//! notifications with auto-dismiss timing and camera-pan on click.

use bevy::ecs::event::Event;
use bevy::prelude::*;
use std::time::Instant;

use crate::plugins::music::{UiSoundKind, UiSoundRequestQueue};
use crate::ui::animations::{ToastKind, ToastMessage, ToastQueue};

/// Notification categories for game events.
///
/// Used to classify notifications and drive visual/icon treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationCategory {
    /// Combat-related alerts (battle started, fleet destroyed, etc.)
    Combat,
    /// Colony construction events (building complete, queue empty, etc.)
    Construction,
    /// Research and technology events (breakthrough, unlock available, etc.)
    Research,
    /// Resource-related alerts (shortage, surplus, production halt)
    Resource,
    /// Fleet movement events (arrival, departure, transfer complete)
    Fleet,
    /// Diplomatic events (contact made, treaty offered, etc.)
    Diplomacy,
    /// General system events (save, load, settings change)
    System,
    /// Archaeology / anomaly discovery events
    Discovery,
    /// Tutorial/hint events
    Tutorial,
}

/// Categories for game notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationKind {
    /// Routine information — auto-dismissed after 30s.
    Info,
    /// Important but not urgent — persists until acknowledged.
    Warning,
    /// Critical alerts — persisted, potential sound cue.
    Critical,
}

impl NotificationKind {
    pub fn should_auto_dismiss(&self) -> bool {
        matches!(self, NotificationKind::Info)
    }

    pub fn tint(&self) -> bevy_egui::egui::Color32 {
        match self {
            NotificationKind::Info => crate::ui::theme::RP_BLUE,
            NotificationKind::Warning => crate::ui::theme::AMBER,
            NotificationKind::Critical => crate::ui::theme::RED,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NotificationKind::Info => "INFO",
            NotificationKind::Warning => "WARNING",
            NotificationKind::Critical => "CRITICAL",
        }
    }
}

/// A single notification entry.
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u64,
    pub kind: NotificationKind,
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    /// World entity associated with this notification (click to pan camera).
    /// `None` means no camera target.
    pub entity: Option<Entity>,
    pub arrived_at: Instant,
    /// When true, warning/critical notifications are considered acknowledged
    /// and stop flashing.
    pub acknowledged: bool,
}

impl Notification {
    pub fn should_auto_dismiss(&self) -> bool {
        self.kind.should_auto_dismiss() && !self.acknowledged
    }

    pub fn age_seconds(&self) -> f64 {
        self.arrived_at.elapsed().as_secs_f64()
    }

    pub fn is_expired(&self) -> bool {
        self.age_seconds() > 30.0 && self.should_auto_dismiss()
    }
}

/// Global notification queue — persists across frames.
#[derive(Resource, Default)]
pub struct NotificationQueue {
    pub items: Vec<Notification>,
    pub next_id: u64,
}

impl NotificationQueue {
    /// Add a new notification. Returns the assigned id.
    pub fn push(&mut self, notification: Notification) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let mut n = notification;
        n.id = id;
        self.items.push(n);
        id
    }

    /// Remove a notification by id.
    pub fn remove(&mut self, id: u64) {
        self.items.retain(|n| n.id != id);
    }

    /// Remove all expired auto-dismiss notifications.
    pub fn prune_expired(&mut self) {
        self.items.retain(|n| !n.is_expired());
    }

    /// Acknowledge a notification (stops flashing for warning/critical).
    pub fn acknowledge(&mut self, id: u64) {
        if let Some(n) = self.items.iter_mut().find(|n| n.id == id) {
            n.acknowledged = true;
        }
    }
}

/// Emit this event to enqueue a notification.
#[derive(Debug, Clone, Event)]
pub struct EmitNotification {
    pub kind: NotificationKind,
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    pub entity: Option<Entity>,
    /// Whether to play a sound effect (only for critical/warning).
    pub play_sound: bool,
}

impl EmitNotification {
    pub fn info(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::System,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    pub fn warning(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Warning,
            category: NotificationCategory::System,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn critical(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Critical,
            category: NotificationCategory::System,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn with_entity(mut self, entity: Entity) -> Self {
        self.entity = Some(entity);
        self
    }

    pub fn with_category(mut self, category: NotificationCategory) -> Self {
        self.category = category;
        self
    }

    // ─── Combat notifications ───────────────────────────────────────────────

    pub fn combat_alert(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Critical,
            category: NotificationCategory::Combat,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn combat_info(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Combat,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    // ─── Construction notifications ─────────────────────────────────────────

    pub fn construction_complete(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Construction,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    pub fn construction_urgent(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Warning,
            category: NotificationCategory::Construction,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    // ─── Research notifications ────────────────────────────────────────────

    pub fn research_breakthrough(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Research,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn research_available(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Research,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    // ─── Resource notifications ─────────────────────────────────────────────

    pub fn resource_shortage(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Warning,
            category: NotificationCategory::Resource,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn resource_critical(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Critical,
            category: NotificationCategory::Resource,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn resource_surplus(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Resource,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    // ─── Fleet notifications ────────────────────────────────────────────────

    pub fn fleet_arrived(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Fleet,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    pub fn fleet_departed(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Fleet,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    pub fn fleet_transfer_complete(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Fleet,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }

    // ─── Diplomacy notifications ─────────────────────────────────────────────

    pub fn diplomacy_contact(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Diplomacy,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    pub fn diplomacy_treaty(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Warning,
            category: NotificationCategory::Diplomacy,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: true,
        }
    }

    // ─── Tutorial notifications ──────────────────────────────────────────────

    pub fn tutorial_hint(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: NotificationKind::Info,
            category: NotificationCategory::Tutorial,
            title: title.into(),
            body: body.into(),
            entity: None,
            play_sound: false,
        }
    }
}

/// Plugin that manages game events and notifications.
pub struct GameEventsPlugin;

impl Plugin for GameEventsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotificationQueue>()
            .init_resource::<NotificationHistory>()
            .init_resource::<ToastQueue>()
            .add_observer(on_emit_notification)
            .add_systems(Update, Self::prune_expired);
    }
}

impl GameEventsPlugin {
    fn prune_expired(mut queue: ResMut<NotificationQueue>) {
        queue.prune_expired();
    }
}

/// Observer: handle EmitNotification events and enqueue them.
fn on_emit_notification(
    event: On<EmitNotification>,
    mut queue: ResMut<NotificationQueue>,
    mut history: ResMut<NotificationHistory>,
    mut sound_queue: ResMut<UiSoundRequestQueue>,
    mut toast_queue: ResMut<ToastQueue>,
) {
    let notification = Notification {
        id: 0, // assigned by queue.push
        kind: event.kind,
        category: event.category,
        title: event.title.clone(),
        body: event.body.clone(),
        entity: event.entity,
        arrived_at: Instant::now(),
        acknowledged: false,
    };
    queue.push(notification.clone());

    // Add to history (newest-first, cap at MAX_HISTORY)
    history.push(notification.clone());

    // Push a toast for every notification
    let toast_kind = match event.kind {
        NotificationKind::Info => ToastKind::Info,
        NotificationKind::Warning => ToastKind::Warning,
        NotificationKind::Critical => ToastKind::Error,
    };
    let toast = ToastMessage::new(
        format!("{}: {}", event.title, event.body),
        toast_kind,
    );
    toast_queue.push(toast);

    // Queue sound effect if requested
    if event.play_sound {
        let sound_kind = match event.kind {
            NotificationKind::Info => UiSoundKind::NotificationInfo,
            NotificationKind::Warning => UiSoundKind::NotificationWarning,
            NotificationKind::Critical => UiSoundKind::NotificationCritical,
        };
        sound_queue.0.push(sound_kind);
    }
}

/// Maximum notifications to keep in history.
const MAX_HISTORY: usize = 100;

/// Notification history log — maintains the last 100 notifications separately
/// from the active queue. Persists even after notifications are dismissed.
#[derive(Resource, Default)]
pub struct NotificationHistory {
    pub items: Vec<Notification>,
}

impl NotificationHistory {
    fn push(&mut self, notification: Notification) {
        self.items.push(notification);
        // Keep only the last MAX_HISTORY items
        if self.items.len() > MAX_HISTORY {
            self.items.remove(0);
        }
    }

    /// Get all history items, newest first.
    pub fn get_recent(&self, count: usize) -> Vec<&Notification> {
        self.items.iter().rev().take(count).collect()
    }
}