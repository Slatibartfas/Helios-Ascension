//! Bridges — turn domain events into `Message<SfxEvent>`.
//!
//! Two bridges today:
//!
//! - [`ui::ui_sfx_bridge`] — drains [`UiSfxRequest`]s (a
//!   lightweight sub-bus the UI callsites write into) and
//!   emits one `SfxEvent` per request. The UI doesn't import
//!   `SfxEvent` directly — it imports [`UiSfxRequest`] from
//!   here, which is the modder-stable surface.
//! - [`notifications::notification_sfx_bridge`] — queries
//!   `Added<ActiveNotification>` and emits a chime per
//!   coalesced toast. Bypasses the per-raw-event stream on
//!   purpose: a probe-lost flood that produces one toast
//!   yields one chime, not five.
//!
//! Adding a new bridge is the canonical way to wire a new
//! domain (construction, research, fleet, etc.) — see the
//! roadmap in `/memories/session/plan.md` for the inventory.

use bevy::prelude::*;

use super::SfxCueId;

/// UI-side sub-bus. The UI writes one of these per click /
/// tab / panel event; the [`ui::ui_sfx_bridge`] drains it and
/// forwards to the SFX backend.
///
/// **Why a separate message type**: keeps the `SfxPlugin`
/// import surface narrow (UI files don't need to know about
/// `SfxEvent` or `SfxRegistry`) and gives us a stable modder
/// boundary — adding a new UI cue means adding a variant
/// here *and* an entry in the manifest, with no other
/// coupling.
#[derive(Debug, Clone, Copy, Message)]
pub struct UiSfxRequest(pub SfxCueId);

pub mod notifications;
pub mod ui;
