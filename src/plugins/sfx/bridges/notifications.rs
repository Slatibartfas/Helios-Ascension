//! Notification → SFX bridge.
//!
//! Plays [`SfxCueId::NotificationChime`] once per **coalesced**
//! toast — not once per raw event. We use
//! `Query<Entity, Added<ActiveNotification>>` to detect newly
//! inserted toast entities, which means we automatically
//! benefit from the existing coalesce logic in
//! `src/ui/notifications/systems/coalesce.rs` (a probe-loss
//! storm that produces 1 toast yields 1 chime, not 50).
//!
//! ## Why `Added<>` instead of `MessageReader<NotificationEvent>`?
//!
//! The notification bus emits one `NotificationEvent` per
//! *raw* event. The coalesce layer folds duplicates into a
//! single `ActiveNotification` entity. We want one chime per
//! *visible toast*, so `Added<ActiveNotification>` is the
//! correct query — it sees the entity the moment it's
//! inserted, which is exactly when the player sees a new
//! toast appear on screen.
//!
//! ## Cooldown
//!
//! The `notifications.chime` cue has `cooldown_ms: 1000` in
//! the manifest. Even with that, a frame that produced 3
//! toasts (e.g. `probe_lost` + `rover_stuck` + `crew_injured`
//! that didn't coalesce) would still produce 3 chimes. The
//! cooldown caps it at 1 chime/sec. The visual toasts still
//! stack normally — only audio is throttled.

use bevy::prelude::*;

use crate::ui::notifications::components::ActiveNotification;

use super::super::{SfxBus, SfxCueId, SfxEvent};

/// `Update` system — emit one chime per newly-spawned toast.
///
/// Order in the SFX chain: runs *after* `sync_sfx_bus_volume`
/// so the bus's `Notifications` category volume reflects the
/// latest `NotificationSettings::global_enabled` toggle, and
/// *before* `play_sfx_system` so the `SfxEvent` lands in the
/// same drain as any UI bridge requests from the same frame.
pub fn notification_sfx_bridge(
    new_toasts: Query<Entity, Added<ActiveNotification>>,
    bus: Res<SfxBus>,
    mut sfx_events: MessageWriter<SfxEvent>,
) {
    if !bus.is_audible(super::super::SfxCategory::Notifications) {
        // Player has muted notifications (or the master SFX
        // slider is at 0). Don't even queue the events — the
        // cooldown system would block them anyway, but
        // skipping here keeps the SfxEvent buffer clean.
        return;
    }

    let mut count = 0usize;
    for _entity in new_toasts.iter() {
        sfx_events.write(SfxEvent(SfxCueId::NotificationChime));
        count += 1;
    }
    if count > 0 {
        debug!("sfx: notification bridge emitted {count} chime(s)");
    }
}
