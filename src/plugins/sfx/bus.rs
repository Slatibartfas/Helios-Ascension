//! `SfxBus` — per-category volume routing.
//!
//! The bus is the integration point between the *persisted*
//! player preferences ([`PersistentSettings::sfx_volume`]) and
//! the *per-category* toggles that already exist in
//! [`NotificationSettings::per_category[*].sound_on`] (defined
//! at `src/ui/notifications/settings.rs:35-51`).
//!
//! ## Volume composition
//!
//! The final volume that `play_sfx_system` passes to
//! `AudioPlayer` is:
//!
//! ```text
//!   final = sfx_master × category_volume × cue.default_volume
//! ```
//!
//! where:
//! - `sfx_master` ∈ [0, 1] — the player's `PersistentSettings`
//!   slider (already persisted, already wired in the settings
//!   UI; this PR is the first consumer).
//! - `category_volume` ∈ {0, 1} — 0 when the matching
//!   notification category's `sound_on` toggle is off (or when
//!   the SFX category has no notification analogue), 1
//!   otherwise.
//! - `cue.default_volume` ∈ [0, 1] — the per-cue scale from the
//!   manifest (authored per-cue to balance "always-loud"
//!   notifications against "easy-to-miss" UI ticks).
//!
//! Categories without a notification analogue (Ui, Camera,
//! TimeControl, Launch, Persistence) always have
//! `category_volume = 1.0` — they're not toggleable, only the
//! master slider affects them.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::ui::launch::userdata::PersistentSettings;
use crate::ui::notifications::settings::NotificationSettings;

use super::SfxCategory;

/// Per-category volume bus.
///
/// Phase 1 only wires `Ui` and `Notifications` sinks; the
/// remaining categories are reserved for later PRs and have a
/// default of `1.0` (always-on) until their bridges land.
#[derive(Resource, Debug, Clone)]
pub struct SfxBus {
    /// Master volume from `PersistentSettings::sfx_volume`.
    /// Updated each frame by [`sync_sfx_bus_volume`].
    pub master: f32,
    /// Per-category category_volume. Updated each frame.
    pub categories: HashMap<SfxCategory, f32>,
}

impl Default for SfxBus {
    fn default() -> Self {
        let mut categories = HashMap::new();
        // Phase 1: every category defaults to 1.0 (full volume).
        // Per-category mute arrives when the UI checkbox is
        // wired; the storage field already exists on
        // NotificationSettings but the runtime lookup is new.
        for cat in [
            SfxCategory::Ui,
            SfxCategory::Construction,
            SfxCategory::Research,
            SfxCategory::Engineering,
            SfxCategory::Shipbuilding,
            SfxCategory::Fleets,
            SfxCategory::Notifications,
            SfxCategory::Economy,
            SfxCategory::Colony,
            SfxCategory::Survey,
            SfxCategory::Camera,
            SfxCategory::TimeControl,
            SfxCategory::Launch,
            SfxCategory::Persistence,
            SfxCategory::Personnel,
        ] {
            categories.insert(cat, 1.0);
        }
        Self {
            master: 1.0,
            categories,
        }
    }
}

impl SfxBus {
    /// Compose the final linear volume for a cue in a given
    /// category. Clamps to `[0.0, 1.0]` defensively (a master
    /// slider at the very top + a 1.0 cue = exactly 1.0; a
    /// future modded manifest with `default_volume: 1.5`
    /// wouldn't overflow into the audio backend).
    pub fn volume_for(&self, category: SfxCategory, cue_default: f32) -> f32 {
        let cat = self.categories.get(&category).copied().unwrap_or(1.0);
        (self.master * cat * cue_default.clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }

    /// Resolve whether a category is currently *audible* (i.e.
    /// neither master-muted nor category-muted). Used by
    /// bridges to short-circuit `MessageWriter<SfxEvent>` so we
    /// don't even queue plays that would be silenced.
    pub fn is_audible(&self, category: SfxCategory) -> bool {
        if self.master <= 0.0 {
            return false;
        }
        match self.categories.get(&category) {
            Some(v) => *v > 0.0,
            None => true,
        }
    }
}

/// `Update` system — runs first in the SFX chain. Refreshes
/// `SfxBus::master` from the persistent settings and the
/// per-category volumes from the notification settings.
///
/// Cheap: `O(num_categories)` lookups in two `HashMap`s. Runs
/// every frame even when the bus is empty (no-ops when both
/// resources are unchanged).
pub fn sync_sfx_bus_volume(
    settings: Option<Res<PersistentSettings>>,
    notif_settings: Option<Res<NotificationSettings>>,
    mut bus: ResMut<SfxBus>,
) {
    let Some(settings) = settings else {
        return;
    };
    bus.master = settings.sfx_volume.clamp(0.0, 1.0);

    let Some(notif) = notif_settings else {
        return;
    };

    for (cat, slot) in bus.categories.iter_mut() {
        let v = match cat.notification_category_for() {
            // Phase 1: only Notifications uses its own per-category
            // toggle. The notification chime lives in the
            // Notifications category and is muted when the global
            // notification master switch is off.
            Some("construction.complete")
            | Some("research.tech_unlocked")
            | Some("economy.stockpile_critical")
            | Some("survey.mission_complete") => {
                if !notif.global_enabled {
                    0.0
                } else {
                    let id = crate::ui::notifications::settings::NotificationCategoryId::from(
                        cat.notification_category_for().unwrap_or(""),
                    );
                    notif
                        .per_category
                        .get(&id)
                        .map(|p| if p.sound_on { 1.0 } else { 0.0 })
                        .unwrap_or(1.0)
                }
            }
            Some(_) => {
                // Any other category mapped to a notification id
                // inherits the same `sound_on` toggle for now.
                if !notif.global_enabled {
                    0.0
                } else {
                    1.0
                }
            }
            None => 1.0, // UI / Camera / etc. — not toggleable, only master.
        };
        *slot = v;
    }

    // Special-case: the Notifications category always plays when
    // the master SFX slider is up, regardless of per-category
    // settings — the chime is the player-facing feedback for
    // every notification, including ones whose category the
    // player may have muted via the category toggle.
    // (This is the same "master wins" semantics as the egui
    // music slider: the master bus is the master bus.)
    let notifications_value = if !notif.global_enabled {
        0.0
    } else if bus.master > 0.0 {
        // Honour the global notification master toggle: if
        // the player has globally silenced notifications,
        // silence the chime too. Per-category `sound_on`
        // toggles do *not* mute the chime (they mute the
        // *toast*, which is a separate visual surface).
        1.0
    } else {
        0.0
    };
    if let Some(slot) = bus.categories.get_mut(&SfxCategory::Notifications) {
        *slot = notifications_value;
    }
}
