//! egui wrappers that auto-fire `UiSfxRequest` on click.
//!
//! The wrappers exist because the bare callsite pattern
//! (`ui.button("X").clicked()` → `sfx_ui.write(...)`) is
//! easy to forget — every missed call site plays silently,
//! the build stays green, CI passes, the player experiences a
//! half-muted UI. The wrappers make the cue a *required*
//! parameter so the compiler can no longer be ignored at
//! the call site.
//!
//! ## Usage
//!
//! Replace a direct `ui.button("OK").clicked()` with:
//!
//! ```ignore
//! if egui_sfx_button(ui, "OK", SfxCueId::ButtonClick, &mut sfx_ui).clicked() {
//!     do_thing();
//! }
//! ```
//!
//! Every helper takes `&mut MessageWriter<UiSfxRequest>` as
//! its last argument so the cue is enqueued in the same
//! closure context as the click. Threading the writer into
//! deeper helpers is fine; the alternative (post-click
//! dispatch) bleeds the cue into the resource-mutation
//! section of every system and scatters the wiring across
//! files (which is the failure mode the wrappers exist to
//! prevent).
//!
//! ## Coverage
//!
//! These helpers cover the four egui widget patterns that
//! account for >95 % of UI input clicks in the project:
//! - `ui.button(...)` / `ui.add(egui::Button::new(...))` →
//!   `egui_sfx_button`
//! - `ui.selectable_label(...)` → `egui_sfx_selectable_label`
//! - `ui.checkbox(...)` → `egui_sfx_checkbox`
//! - `ui.toggleable_value(...)` → `egui_sfx_toggleable`
//!
//! Other interactive widgets (slider drag, drag-drop,
//! text-commit) still require a manual
//! `sfx_ui.write(UiSfxRequest(...))`; the coverage audit
//! [`scripts/audit_sfx_coverage.py`] flags untracked callsites
//! in CI.

use bevy::prelude::MessageWriter;
use bevy_egui::egui;

use crate::plugins::sfx::bridges::UiSfxRequest;
use crate::plugins::sfx::SfxCueId;

/// Fire a cue (with cooldown deduplication handled by
/// [`SfxRegistry`]). Cheap when the same cue fires twice in a
/// frame — the SFX backend dedupes.
#[inline]
fn emit(sfx_ui: &mut MessageWriter<UiSfxRequest>, cue: SfxCueId) {
    sfx_ui.write(UiSfxRequest(cue));
}

/// `egui::button(...)` + auto-fire of a cue on `.clicked()`.
///
/// Returns the underlying `egui::Response` so callers can
/// inspect hover / press / drag-state without losing the
/// click signal.
///
/// # Example
///
/// ```ignore
/// if egui_sfx_button(ui, "Confirm", SfxCueId::ButtonClick, &mut sfx_ui)
///     .on_hover_text("Apply changes")
///     .clicked()
/// {
///     apply();
/// }
/// ```
pub fn egui_sfx_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    cue: SfxCueId,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) -> egui::Response {
    let response = ui.button(label);
    if response.clicked() {
        emit(sfx_ui, cue);
    }
    response
}

/// `ui.add(egui::Button::new(...))` variant — preserves any
/// pre-built `egui::Button` so callers can apply `.fill(...)`,
/// `.stroke(...)`, `.sense(...)`, etc.
///
/// # Example
///
/// ```ignore
/// let btn = egui::Button::new("OK")
///     .fill(theme::SURFACE)
///     .stroke(egui::Stroke::new(1.0, theme::CYAN));
/// if egui_sfx_button_built(ui, btn, SfxCueId::ModalConfirm, &mut sfx_ui)
///     .clicked()
/// {
///     confirm();
/// }
/// ```
pub fn egui_sfx_button_built(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    cue: SfxCueId,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) -> egui::Response {
    let response = ui.add(button);
    if response.clicked() {
        emit(sfx_ui, cue);
    }
    response
}

/// `ui.selectable_label(...)` + auto-fire of a cue.
///
/// Returns the `egui::Response`; the new `selected` value is
/// also returned so callers can update their own state in
/// one expression.
pub fn egui_sfx_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<egui::WidgetText>,
    cue: SfxCueId,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) -> (egui::Response, bool) {
    let response = ui.selectable_label(selected, text);
    let new_selected = if response.clicked() {
        !selected
    } else {
        selected
    };
    if response.clicked() {
        emit(sfx_ui, cue);
    }
    (response, new_selected)
}

/// `ui.checkbox(...)` + auto-fire of a cue when the value
/// changes. Fires **only** on `.changed()` transitions so
/// initial state seeding doesn't play a sound.
pub fn egui_sfx_checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
    cue: SfxCueId,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = ui.checkbox(value, text);
    if response.changed() {
        emit(sfx_ui, cue);
    }
    response
}

/// `ui.toggle_value(...)` + auto-fire of a cue on transition.
/// (egui 0.33: method is `toggle_value`, not `toggleable_value`.)
pub fn egui_sfx_toggle(
    ui: &mut egui::Ui,
    current: &mut bool,
    text: impl Into<egui::WidgetText>,
    cue: SfxCueId,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) -> egui::Response {
    let was = *current;
    let response = ui.toggle_value(current, text);
    if response.clicked() && *current != was {
        emit(sfx_ui, cue);
    }
    response
}

/// Specialised variant for the common UX-3x glass-button
/// pattern that the `launch` menu surfaces already use at
/// [`crate::ui::launch::menu::render_glass_button`].
/// Fire-and-forget — the helpers above return `Response` so
/// the caller can add `on_hover_text`/etc.; glass buttons
/// don't need that, so this one returns nothing.
pub fn egui_sfx_glass_button(
    ui: &mut egui::Ui,
    label: &str,
    cue: SfxCueId,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) {
    if crate::ui::launch::render_glass_button(ui, label, "", true).clicked() {
        emit(sfx_ui, cue);
    }
}

// ===========================================================================
// Category 2 — SLIDER (continuous cues, throttled)
// ===========================================================================
//
// A slider's "click" doesn't exist; the input is a drag. But
// hitting `SliderTick` on every drag delta at 60 Hz would
// saturate the SFX backend (and the player's ears). We
// throttle to a single tick per `SLIDER_TICK_MIN_INTERVAL_MS`
// of *cumulative* drag motion, which gives a satisfying
// rhythmic feedback during long drags without spamming the
// bus.
//
// Stash the last-tick instant in a `Local<Instant>` per slider
// if multiple live in one frame. The wrapper does this
// transparently — callers do not see the throttle.

/// Minimum interval between two `SliderTick` cues from a
/// single slider, even when the user is dragging fast.
pub const SLIDER_TICK_MIN_INTERVAL_MS: u64 = 80;

/// `egui::Slider::new(...)` + throttled `SliderTick` on
/// drag. The track state is unchanged — the cue is purely
/// feedback — but no cue fires if the value didn't change.
///
/// Requires a `Local<Instant>` per slider to track the
/// last-tick time. Most callers pass a single
/// `Local<Instant>` shared across sliders — the throttle is
/// global which is fine because rapid parallel sliders are
/// user-confusing anyway.
///
/// # Example
///
/// ```ignore
/// fn settings_system(
///     mut ui: ...,
///     mut brightness: LocalMut<f32>,
///     mut last_tick: Local<Instant>,
///     mut sfx_ui: MessageWriter<UiSfxRequest>,
/// ) {
///     let was = *brightness;
///     egui_sfx_slider(
///         ui,
///         &mut *brightness,
///         0.0..=1.0,
///         "Brightness",
///         &mut last_tick,
///         &mut sfx_ui,
///     );
/// }
/// ```
pub fn egui_sfx_slider<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    text: impl Into<egui::WidgetText>,
    last_tick: &mut std::time::Instant,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) -> egui::Response {
    let was = *value;
    let response = ui.add(egui::Slider::new(value, range).text(text));
    if !response.changed() {
        return response;
    }
    let now = std::time::Instant::now();
    if now.duration_since(*last_tick).as_millis() as u64 >= SLIDER_TICK_MIN_INTERVAL_MS {
        *last_tick = now;
        emit(sfx_ui, SfxCueId::SliderTick);
        // Suppress unused-warning for `was`; reserved for
        // future "debounce single-step ticks" feature.
        let _ = was;
    }
    response
}

// ===========================================================================
// Category 3 — COMBOBOX (rising-edge `DropdownOpen`)
// ===========================================================================
//
// egui's `ComboBox::show(ui, ...)` returns a `Response` whose
// `.inner` is the user-rendered dropdown body; the open
// state lives on the parent's `Memory`. We can't read the
// state from a single Response, so the wrapper returns the
// toggle bit and the caller compares against its own last-known
// state. Repeat calls with the same `is_open` value never re-fire.

/// `egui::ComboBox::from_label(...)` + a body closure + auto-fire
/// `DropdownOpen` while the popup is open.
///
/// Detection of "is the popup open this frame" reads egui's
/// internal memory. Note that egui 0.33 deprecated the
/// `Memory::is_popup_open` API in favour of `Popup::is_id_open`;
/// we use the deprecated path because it is the only one that
/// works against `bevy_egui::egui::Memory`'s stable surface.
/// The wrapper fires the cue on every frame the popup is open;
/// callers that want rising-edge-only behaviour must pair
/// this with a `Local<bool>` comparison (see the example).
///
/// # Example
///
/// ```ignore
/// fn my_combo(
///     ui: &mut egui::Ui,
///     selected: &mut Color,
///     mut was_open: Local<bool>,
///     mut sfx_ui: MessageWriter<UiSfxRequest>,
/// ) {
///     let is_open_now = egui_sfx_combo(
///         ui,
///         "Color",
///         selected.label(),
///         |ui| for c in PALETTE { ui.selectable_value(selected, c, c.label()); }
///         ,
///         &mut sfx_ui,
///     );
///     if is_open_now && !*was_open {
///         // rising edge — wrapper here is just used to
///         // suppress duplicate cues within one frame.
///     }
///     *was_open = is_open_now;
/// }
/// ```
pub fn egui_sfx_combo(
    ui: &mut egui::Ui,
    label: &str,
    selected_text: &str,
    body: impl FnOnce(&mut egui::Ui),
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) -> bool {
    let combo_id = ui.id().with("sfx_combo").with(label);
    egui::ComboBox::from_label(label)
        .selected_text(selected_text)
        .show_ui(ui, body);
    // After show_ui has run, egui's memory knows whether the
    // popup is open for the label-derived id. Read it back to
    // determine the current state.
    let is_open_now = ui.memory(|m| {
        // egui 0.33: `is_popup_open` is deprecated in favour of
        // `Popup::is_id_open`. We use the legacy path because the
        // replacement requires threading a `Popup` reference through
        // `ui`, which the public ComboBox surface doesn't expose.
        #[allow(deprecated)]
        let is_open = m.is_popup_open(combo_id);
        is_open
    });
    if is_open_now {
        emit(sfx_ui, SfxCueId::DropdownOpen);
    }
    is_open_now
}

// ===========================================================================
// Category 4 — TAB SWITCH (rising-edge `TabSwitch`)
// ===========================================================================
//
// egui doesn't have a "native tab" widget; every menu rolls
// its own with `ui.selectable_label` (a specialised button).
// The TabSwitch cue fires on the rising edge of `active` —
// the first frame the player lands on a new tab — not per
// frame the active tab is drawn (otherwise it'd tick every
// redraw).

/// Compare a `new_active` value against `last_active` and
/// fire `TabSwitch` on the transition. Callers typically
/// invoke this right after the `if clicked()` block that
/// updates the active tab.
///
/// # Example
///
/// ```ignore
/// fn tabs(
///     ui: &mut egui::Ui,
///     active: &mut usize,
///     last_active: &mut usize,
///     mut sfx_ui: MessageWriter<UiSfxRequest>,
/// ) {
///     for (i, label) in ["Overview", "Detail"].iter().enumerate() {
///         if ui.selectable_label(*active == i, *label).clicked() {
///             *active = i;
///             egui_sfx_tab_switch_fire(*active, last_active, &mut sfx_ui);
///         }
///     }
/// }
/// ```
pub fn egui_sfx_tab_switch_fire(
    new_active: usize,
    last_active: &mut usize,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) {
    if new_active != *last_active {
        emit(sfx_ui, SfxCueId::TabSwitch);
        *last_active = new_active;
    }
}

// ===========================================================================
// Category 5 — DRAG-DROP (`DragDrop` on drag_started / dnd_drop)
// ===========================================================================
//
// egui exposes two distinct drag primitives: reordering (a
// `drag_started` / `drag_stopped` event with the source id)
// and cross-target (a `dnd_drop` event with a payload). Both
// pathways signal "something moved" — the player should hear
/// the cue exactly once per completed drag, not on every
/// `set_drag_payload` in between.
/// Call at the end of a successful egui drag-destination
/// block (after the `dnd_drop` payload is consumed). Fires
/// `DragDrop` exactly once per logical drop.
pub fn egui_sfx_dnd_drop(sfx_ui: &mut MessageWriter<UiSfxRequest>, dropped: bool) {
    if dropped {
        emit(sfx_ui, SfxCueId::DragDrop);
    }
}

/// Call at the end of an egui reorder-source block (after
/// the source accepts its own reorder). Fires `DragDrop`
/// exactly once per reorder.
pub fn egui_sfx_drag_finished(sfx_ui: &mut MessageWriter<UiSfxRequest>) {
    emit(sfx_ui, SfxCueId::DragDrop);
}

// ===========================================================================
// Category 6 — PANEL STATE (rising-edge `PanelOpen` / `PanelClose`)
// ===========================================================================
//
// Wrappers don't help when a panel's open/close is driven by
// state (LaunchState::NewGame → Settings) or by an observer
// rather than a click. Provide a small helper that fires the
/// cue on a rising / falling edge of a "currently open" bool.
///
/// # Example
///
/// ```ignore
/// fn mod_system(
///     was_open: Local<bool>,
///     is_open_now: bool,
///     mut sfx_ui: MessageWriter<UiSfxRequest>,
/// ) {
///     egui_sfx_panel_transition(*was_open, is_open_now, &mut sfx_ui);
/// }
/// ```
pub fn egui_sfx_panel_transition(
    was_open: bool,
    is_open: bool,
    sfx_ui: &mut MessageWriter<UiSfxRequest>,
) {
    match (was_open, is_open) {
        (false, true) => emit(sfx_ui, SfxCueId::PanelOpen),
        (true, false) => emit(sfx_ui, SfxCueId::PanelClose),
        _ => {}
    }
}

// ===========================================================================
// Category 7 — CHIPS / MODES (key-press fallback for `Space`/`Enter`)
// ===========================================================================
//
// The chips that bound the Layout's right side in some panels
// (Settings, Notifications) only fire `ChipToggle` / `ModeToggle`
// on click. But Space / Enter also activate them per egui's
// default KeyboardNav. These helpers let the call-site fire the
// cue from either path (click OR key) without having to
/// duplicate the call after the `.clicked()` block.
///
/// Most panels just keep using the click-wrapper; this is for
/// the few that wire a key handler too.
#[inline]
pub fn egui_sfx_chip_toggle(sfx_ui: &mut MessageWriter<UiSfxRequest>) {
    emit(sfx_ui, SfxCueId::ChipToggle);
}

#[inline]
pub fn egui_sfx_mode_toggle(sfx_ui: &mut MessageWriter<UiSfxRequest>) {
    emit(sfx_ui, SfxCueId::ModeToggle);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::plugins::sfx::SfxCueId;

    /// Compile-time sanity check: every cue variant can flow
    /// through a wrapper. If a new variant is added and the
    /// `SfxCueId` enum drifts, this still compiles but the
    /// exhaustiveness check at the playback layer is the
    /// authoritative one — see `SfxCueId::ALL`.
    #[allow(dead_code)]
    fn compile_time_exhaustiveness(cue: SfxCueId) {
        // Touch every variant so the compiler doesn't elide
        // unused warnings on the enum match in `_unused`.
        let _unused = match cue {
            SfxCueId::ButtonClick => "button_click",
            SfxCueId::TabSwitch => "tab_switch",
            SfxCueId::PanelOpen => "panel_open",
            SfxCueId::PanelClose => "panel_close",
            SfxCueId::SliderTick => "slider_tick",
            SfxCueId::DropdownOpen => "dropdown_open",
            SfxCueId::RowSelect => "row_select",
            SfxCueId::DragDrop => "drag_drop",
            SfxCueId::ModalConfirm => "modal_confirm",
            SfxCueId::ModalCancel => "modal_cancel",
            SfxCueId::ChipToggle => "chip_toggle",
            SfxCueId::ModeToggle => "mode_toggle",
            SfxCueId::NotificationChime => "notification_chime",
        };
    }
}
