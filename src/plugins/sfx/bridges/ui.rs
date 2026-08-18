//! UI → SFX bridge.
//!
//! Drains [`UiSfxRequest`]s (written by UI callsites at the
//! point of click/tab/panel-toggle) and forwards each one as
//! an [`SfxEvent`]. Runs every frame; cost is `O(n)` in the
//! number of UI events that frame (typically 0–3).
//!
//! ## Call-site pattern
//!
//! UI files add a single line at the point of action:
//!
//! ```ignore
//! use crate::plugins::sfx::bridges::UiSfxRequest;
//!
//! fn my_button(ui: &mut egui::Ui, mut sfx: MessageWriter<UiSfxRequest>) {
//!     if ui.button("Confirm").clicked() {
//!         sfx.write(UiSfxRequest(SfxCueId::ModalConfirm));
//!     }
//! }
//! ```
//!
//! The callsite never touches `SfxBus` or `SfxRegistry`
//! directly — those are owned by the plugin.
//!
//! ## Why not write `SfxEvent` directly?
//!
//! Two reasons:
//! 1. **Compile-time safety**: adding a new cue is a
//!    `SfxCueId` enum variant. UI callsites get a compile
//!    error if they pass an unknown id; with `SfxEvent`
//!    they'd pass an arbitrary `SfxCueId` and silently play
//!    nothing.
//! 2. **Modder boundary**: UI files don't need to know about
//!    `SfxEvent`. `UiSfxRequest` is the public surface; the
//!    internal `SfxEvent` can grow new fields (e.g. position,
//!    pitch) without forcing UI file recompilation.

use bevy::prelude::*;

use super::UiSfxRequest;
use crate::plugins::sfx::{PendingSfxRequests, SfxRequestCollector};

/// `Update` system — drain UI requests, emit `SfxEvent`s.
///
/// Returns silently on an empty buffer (the common case).
/// The `tracing`/`debug!` line is gated on activity so the
/// per-frame log volume is zero in the steady state.
pub fn ui_sfx_bridge(
    mut ui_requests: MessageReader<UiSfxRequest>,
    mut sfx_events: MessageWriter<super::super::SfxEvent>,
) {
    let mut count = 0usize;
    for UiSfxRequest(id) in ui_requests.read() {
        sfx_events.write(super::super::SfxEvent(*id));
        count += 1;
    }
    if count > 0 {
        debug!("sfx: ui bridge forwarded {count} request(s)");
    }
}

/// Drain [`SfxRequestCollector`] and the per-frame
/// [`PendingSfxRequests`](crate::plugins::sfx::PendingSfxRequests)
/// resource (the escape hatches for systems at the 16-parameter
/// `IntoSystem` limit) into the [`UiSfxRequest`] message bus.
///
/// Runs early in the SfxPlugin Update chain, *before*
/// [`ui_sfx_bridge`], so the messages it writes can be drained
/// in the same frame by `ui_sfx_bridge`. The collector empties
/// after this system reads it — the next frame starts fresh.
///
/// **Cost**: `O(n)` in the number of cues pushed that frame.
/// Typical case: 0 cues (collector sits empty and the system
/// is a no-op). Worst observed case in benchmarks: 6 cues per
/// frame on the dashboard's busiest tick.
///
/// **Why a Resource, not a `Local<MessageWriter>`**: see the
/// `SfxRequestCollector` doc comment. The bridge is the single
/// canonical place that converts "queued cue" → "message"; no
/// other system should ever write to `UiSfxRequest` directly
/// from a Resource.
pub fn drain_collector_into_ui_sfx(
    mut collector: ResMut<SfxRequestCollector>,
    mut commands: Commands,
    pending: Option<Res<PendingSfxRequests>>,
    mut ui_requests: MessageWriter<UiSfxRequest>,
) {
    let count = collector.len();
    if count > 0 {
        for cue in collector.drain() {
            ui_requests.write(UiSfxRequest(cue));
        }
        debug!("sfx: drain_collector forwarded {count} queued cue(s) → UiSfxRequest");
    }
    // The PendingSfxRequests resource is only present on frames
    // where a system at the 16-param cap fired a cue via
    // `commands.insert_resource(...)`. Drain + remove it so
    // the next frame starts clean.
    if let Some(pending) = pending {
        for cue in pending.0.iter().copied() {
            ui_requests.write(UiSfxRequest(cue));
        }
        debug!(
            "sfx: drain_pending forwarded {} queued cue(s) → UiSfxRequest",
            pending.0.len()
        );
        commands.remove_resource::<PendingSfxRequests>();
    }
}
