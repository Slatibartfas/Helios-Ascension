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
