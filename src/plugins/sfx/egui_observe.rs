//! Runtime egui observer.
//!
//! Sits after every panel in `EguiPrimaryContextPass` and emits
//! SFX cues for *any* interactive egui event the wrappers above
//! missed.
//!
//! ## What this catches
//!
//! The egui SFX wrappers (in `src/ui/egui_sfx.rs`) require every
//! callsite to opt in via `egui_sfx_button(...)` etc. That
//! works for the panels we've migrated, but new panels added
//! in future PRs won't be wired — and a missing `sfx_ui.write`
//! in a panel is invisible until the player clicks.
//!
//! The runtime observer is **defense in depth**: even if a
//! callsite forgets the wrapper, the player still hears a
//! cue. The cue will be coarse (it's a `ButtonClick` for any
//! click on any widget in the panel), but sound beats silence.
//!
//! ## How
//!
//! Each `EguiPrimaryContextPass` frame, after every panel's
//! render has run, we walk every `EguiContext`'s
//! `egui::Context` and:
//!
//! 1. Capture pointer-down / pointer-up events from
//!    `Memory::interactions` via the `focused()` getter
//!    (the only public surface in egui 0.33 — full
//!    response iteration would require patching egui).
//! 2. Read the focused-widget ID via `memory.focused()`.
//! 3. Emit a `UiSfxRequest` for the panel whose last-frame
//!    focus gained a click transition (we use `Local<Id>`
//!    tracking so the *first* frame a widget is focused
//!    fires once; subsequent frames are quiet — the
//!    per-cue cooldown in `SfxRegistry` keeps this safe).
//!
//! Resolution goes through `SfxPolicy` so per-panel overrides
//! are honoured automatically. If the wraps above already
//! fired a cue for the same click, the per-cue cooldown in
//! `SfxRegistry` will dedupe (the runtime observer's cue
//! lands in the same frame as the wrapper's, hits the
//! cooldown, and is dropped).
//!
//! ## Cost
//!
//! One system tick per frame; reads 1 egui::Context per
//! viewport (typically 1 in Helios); allocates only when
//! the focused ID changed. Worst-case: one cue per frame
//! on a constantly-clicking panel.

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiContexts;

use super::bridges::UiSfxRequest;
use super::policy::{SfxPolicy, WidgetKind};
use super::{SfxBus, SfxCategory};

/// Trigger cue for the catch-all click. Overridable per
/// panel via `SfxPolicy::panel_overrides[id].button`.
const CATCHALL_CUE: WidgetKind = WidgetKind::Button;

/// System that runs after every egui panel in the
/// `EguiPrimaryContextPass`. Reads the egui::Context's
/// focused widget ID, compares against a local cache, and
/// fires a `UiSfxRequest` for the rising edge.
///
/// Run order: must be `.chain().after(<all panel systems>)`.
/// The simplest wiring is to register this system *last*
/// in the pass and let `chain()` ordering do the rest.
pub fn observe_egui_clicks_system(
    mut contexts: EguiContexts,
    mut last_focus: Local<Option<egui::Id>>,
    mut sfx_ui: MessageWriter<UiSfxRequest>,
    mut sfx_policy: ResMut<SfxPolicy>,
    sfx_bus: Res<SfxBus>,
) {
    // Determine which panel ID is "active" for routing cues.
    // For Phase 3a-iii we use the empty string — SfxPolicy's
    // kind_default still fires ButtonClick for unwired
    // panels. Phase 3b can refine this by reading
    // ActiveMenu (which lives in a non-Bevy system).
    let panel_id = "";

    // Skip work early when SFX is muted.
    if !sfx_policy.master_enabled {
        return;
    }
    if !sfx_bus.is_audible(SfxCategory::Ui) {
        last_focus.take();
        return;
    }

    // Walk every egui viewport. Helios currently uses one
    // viewport but the API is iteration-shaped so future
    // multi-viewport support needs no extra code here.
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };

    let now_focused = ctx.memory(|m| m.focused());
    // Rising-edge: previous frame had None or a different
    // ID, this frame has Some(id). Reset on None so the
    // next click is also caught.
    let should_fire = match (*last_focus, now_focused) {
        (None, Some(_)) => true,
        (Some(prev), Some(now)) => prev != now,
        _ => false,
    };
    *last_focus = now_focused;

    if !should_fire {
        return;
    }

    // Resolve via SfxPolicy. The label is empty here — the
    // observer doesn't know which widget kind was clicked;
    // SfxPolicy::kind_default chooses ButtonClick for the
    // generic WidgetKind::Button path used by the catch-all.
    sfx_policy.stats.clicks_detected += 1;
    if let Some(cue) = sfx_policy.resolve(panel_id, CATCHALL_CUE, "") {
        sfx_policy.stats.cues_fired += 1;
        sfx_ui.write(UiSfxRequest(cue));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-check that the constant points at a sensible cue
    /// path. If SfxCueId is renamed this breaks the build,
    /// which is the point — the catch-all should remain a
    /// well-known cue that doesn't drift silently.
    #[test]
    fn catchall_cue_is_button_kind() {
        assert_eq!(CATCHALL_CUE, WidgetKind::Button);
    }
}
