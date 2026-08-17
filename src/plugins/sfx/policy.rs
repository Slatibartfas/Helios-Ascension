//! `SfxPolicy` — single source of truth for per-widget / per-event
//! → `SfxCueId` mappings.
//!
//! ## Why this exists
//!
//! Before `SfxPolicy`, every panel that wanted a SFX had to import
//! `UiSfxRequest` and write a per-callsite `sfx_ui.write(...)` next
//! to each `if ui.button("X").clicked()`. That made the system
//! hard to:
//!
//! - **Audit** — "did the dropdown cue ever fire?" was a grep, not
//!   an answer.
//! - **Reconfigure** — switching the dropdown cue from
//!   `DropdownOpen` to a custom variant required grepping every
//!   panel and updating each callsite.
//! - **Extend** — adding a new `SfxCueId` (`BuildComplete`,
//!   `LaunchCountdown`, etc.) required editing every callsite that
//!   should fire it.
//!
//! `SfxPolicy` flips the direction: the playback layer asks
//! "given that an egui `Response::clicked()` fired in panel X
//! against a widget labelled Y at position Z with kind K, what
//! cue should play?" — and the answer is *one* lookup here.
//!
//! Per-panel `SfxPanelGuard`s ([`crate::ui::egui_sfx`]) read the
//! policy instead of taking a hardcoded `SfxCueId`. Adding a new
//! cue only requires updating this struct + the manifest +
//! regenerating the WAV; no callsite changes flow backwards.
//!
//! ## Mapping resolution order
//!
//! When the runtime observer fires (see `egui_observe.rs`), it
//! resolves the cue to play in this order:
//!
//! 1. **Per-panel override** if the panel's ID appears in
//!    [`panel_overrides`] AND the widget kind has an entry there.
//! 2. **Per-label override** if a heuristic (`starts_with("Cancel")`,
//!    `contains("Delete")`, `ends_with("Quit")`, etc.) classifies
//!    the click — see [`label_match`]. These are pure functions of
//!    the button text; no regex, no I/O.
//! 3. **Per-kind default** in [`kind_default`].
//! 4. **No cue** (returns `None`) — for labels we explicitly want
//!    silent (e.g. the "Hide" toggle in the dev menu).
//!
//! ## Mutability
//!
//! The whole struct is `Resource` and lives in `ResMut` for the
//! settings panel to mutate at runtime. The runtime observer
//! reads via `Res<SfxPolicy>` so reads are lock-free.

use bevy::prelude::*;

use super::SfxCueId;

/// The kind of egui widget that triggered the interaction.
///
/// Classified by the runtime observer based on `egui::Response`
/// properties (rect, sense, modifiers, siblings). The enum is
/// kept small because every variant is one default in
/// [`SfxPolicy::kind_default`] — adding a variant is a deliberate
/// policy decision, not a refactor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidgetKind {
    /// `egui::Button` of any flavour.
    Button,
    /// `egui::SelectableLabel` / `selectable_label`.
    Selectable,
    /// `egui::Checkbox`.
    Checkbox,
    /// `egui::ComboBox::from_label` (already-open popup).
    ComboOpen,
    /// `egui::Slider` / `DragValue` (slider tick cue).
    Slider,
    /// `egui::Tab` strip click (typical custom render).
    Tab,
    /// Anything else that fires a click (`Hyperlink`, image
    /// button, etc.) — gets the default ButtonClick cue.
    Other,
    /// Categorically silenced — the runtime observer never
    /// fires for these (used for diagnostics/debug menu
    /// where audio is unwanted).
    Silent,
}

impl WidgetKind {
    /// Classify based on text heuristics. Used by the runtime
    /// observer when no per-panel override exists. Pure
    /// function; no I/O.
    pub fn classify(text: &str) -> Self {
        let lower = text.to_ascii_lowercase();
        // First check the silencing/heuristic overrides.
        if matches!(lower.as_str(), "") {
            return Self::Silent;
        }
        if lower.contains("cancel") || lower.contains("close") || lower.contains("back") {
            return Self::Button;
        }
        Self::Button
    }
}

/// Top-level config resource. Lives in `World::resource::<SfxPolicy>()`.
///
/// Settings UI (the notifications panel and the launch settings
/// subview) write to this via `ResMut<SfxPolicy>`. The runtime
/// observer reads via `Res<SfxPolicy>`.
#[derive(Resource, Debug, Clone)]
pub struct SfxPolicy {
    /// Master switch — when `false`, no SFX fires at all (the
    /// settings toggle reads as `PersistentSettings::sfx_volume
    /// == 0.0` but this is a faster gate).
    pub master_enabled: bool,

    /// Map from `PanelId` → per-widget-kind cue overrides.
    /// Keys are stable per-panel string IDs (e.g.
    /// `"research"`, `"fleets"`, `"transfer_planner"`). Values
    /// are sparse overrides — missing widget kinds fall through
    /// to [`kind_default`].
    ///
    /// Stored as a `Vec<(String, KindMap)>` rather than a
    /// `HashMap` so the resource serializes deterministically
    /// (matches the snapshot diffing pipeline).
    pub panel_overrides: Vec<(String, KindMap)>,

    /// Active per-frame SFX statistics — observability only.
    /// Updated by the runtime observer every frame; reset
    /// by the playback system after each tick. Hot path
    /// skips the counter increment when `debug!` is off.
    pub stats: SfxStats,
}

impl Default for SfxPolicy {
    fn default() -> Self {
        Self {
            master_enabled: true,
            panel_overrides: default_panel_overrides(),
            stats: SfxStats::default(),
        }
    }
}

/// Sparse per-kind cue map: presence means "override the
/// kind_default for this kind in this panel". Stored as
/// `Vec` to match `serde` deterministic ordering of the
/// surrounding struct.
#[derive(Debug, Clone, Default)]
pub struct KindMap {
    pub button: Option<SfxCueId>,
    pub selectable: Option<SfxCueId>,
    pub checkbox: Option<SfxCueId>,
    pub combo_open: Option<SfxCueId>,
    pub slider: Option<SfxCueId>,
    pub tab: Option<SfxCueId>,
    pub other: Option<SfxCueId>,
}

impl KindMap {
    fn get(&self, kind: WidgetKind) -> Option<SfxCueId> {
        match kind {
            WidgetKind::Button => self.button,
            WidgetKind::Selectable => self.selectable,
            WidgetKind::Checkbox => self.checkbox,
            WidgetKind::ComboOpen => self.combo_open,
            WidgetKind::Slider => self.slider,
            WidgetKind::Tab => self.tab,
            WidgetKind::Other => self.other,
            WidgetKind::Silent => None,
        }
    }
}

/// Per-frame observability counters. Reset at the start of every
/// playback tick so the values logged reflect *this* frame.
/// Hot-path code increments via the runtime observer; release
/// builds pay one atomic add per click.
#[derive(Debug, Default, Clone, Copy)]
pub struct SfxStats {
    pub clicks_detected: u32,
    pub cues_resolved: u32,
    pub cues_fired: u32,
    pub cue_overrides_hit: u32,
    pub heuristic_hits: u32,
}

impl SfxPolicy {
    /// Resolve the cue for a click.
    ///
    /// Used by [`crate::ui::egui_sfx::egui_sfx_panel_button`]
    /// style wrappers and by the runtime observer at
    /// [`crate::plugins::sfx::egui_observe::observe_egui_clicks`].
    ///
    /// Resolution order:
    /// 1. Per-panel + per-kind override (`panel_overrides`).
    /// 2. Label heuristic (via [`resolve_label_match`]).
    /// 3. Per-kind default ([`kind_default`]).
    /// 4. `None` → silent.
    pub fn resolve(&mut self, panel_id: &str, kind: WidgetKind, label: &str) -> Option<SfxCueId> {
        if !self.master_enabled {
            return None;
        }
        // 1. Per-panel override.
        for (id, kinds) in &self.panel_overrides {
            if id == panel_id {
                if let Some(cue) = kinds.get(kind) {
                    self.stats.cue_overrides_hit += 1;
                    self.stats.cues_resolved += 1;
                    return Some(cue);
                }
            }
        }
        // 2. Label heuristic. Returns Some only for strong matches
        //    (cancel/back/close/delete/etc.). Returns None otherwise
        //    so we fall through to step 3 — the kind default.
        if let Some(cue) = resolve_label_match(label) {
            self.stats.heuristic_hits += 1;
            self.stats.cues_resolved += 1;
            return Some(cue);
        }
        // 3. Per-kind default.
        if let Some(cue) = kind_default(kind) {
            self.stats.cues_resolved += 1;
            return Some(cue);
        }
        None
    }
}

/// Per-kind default cues. The comment column is what each cue
/// "feels like" — used as a sanity check when adding new variants.
fn kind_default(kind: WidgetKind) -> Option<SfxCueId> {
    match kind {
        WidgetKind::Button => Some(SfxCueId::ButtonClick),
        WidgetKind::Selectable => Some(SfxCueId::RowSelect),
        WidgetKind::Checkbox => Some(SfxCueId::ChipToggle),
        WidgetKind::ComboOpen => Some(SfxCueId::DropdownOpen),
        WidgetKind::Slider => Some(SfxCueId::SliderTick),
        WidgetKind::Tab => Some(SfxCueId::TabSwitch),
        WidgetKind::Other => Some(SfxCueId::ButtonClick),
        WidgetKind::Silent => None,
    }
}

/// Pure label-based cue overrides. Returns `Some(cue)` only for
/// labels that *clearly* signal a different cue than the
/// kind default — destructive actions, modal cancels, etc.
/// Keep this list SMALL — every entry is one more thing to
/// reason about. False positives are worse than false negatives.
fn resolve_label_match(label: &str) -> Option<SfxCueId> {
    let t = label.trim().to_ascii_lowercase();
    // Empty + whitespace-only labels (decorative widgets) → silent.
    if t.is_empty() {
        return None;
    }
    // Destructive actions / explicit cancels → ModalCancel cue.
    if t.contains("delete")
        || t == "cancel"
        || t.starts_with("cancel ")
        || t.starts_with("cancel\t")
    {
        return Some(SfxCueId::ModalCancel);
    }
    // Confirmations / accepts / OKs / Saves → ModalConfirm cue.
    if t == "save" || t == "ok" || t == "confirm" || t == "apply" || t == "accept" {
        return Some(SfxCueId::ModalConfirm);
    }
    // Open-panel labels → PanelOpen (the open is the meaningful event).
    if t.contains("settings") || t.contains("options") {
        return Some(SfxCueId::PanelOpen);
    }
    if t.contains("close") || t.contains("back") || t.contains("quit") {
        return Some(SfxCueId::PanelClose);
    }
    None
}

/// Default per-panel overrides for v0.5.2. Empty in the
/// baseline — overlays build from here as panels are tuned.
fn default_panel_overrides() -> Vec<(String, KindMap)> {
    vec![
        // `construction` is the only panel that's gone through
        // a full tuning pass; it gets explicit cues for
        // chip-style toggles so construction-mode chips make
        // the right sound.
        (
            "construction".to_string(),
            KindMap {
                checkbox: Some(SfxCueId::ChipToggle),
                ..Default::default()
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_resolves_kind() {
        let mut policy = SfxPolicy::default();
        assert_eq!(
            policy.resolve("any", WidgetKind::Button, "Foo"),
            Some(SfxCueId::ButtonClick)
        );
        assert_eq!(
            policy.resolve("any", WidgetKind::Checkbox, "Foo"),
            Some(SfxCueId::ChipToggle)
        );
        assert_eq!(
            policy.resolve("any", WidgetKind::Slider, "Foo"),
            Some(SfxCueId::SliderTick)
        );
    }

    #[test]
    fn label_heuristic_classifies_cancel_as_modal_cancel() {
        let mut policy = SfxPolicy::default();
        assert_eq!(
            policy.resolve("any", WidgetKind::Button, "Cancel"),
            Some(SfxCueId::ModalCancel)
        );
        assert_eq!(
            policy.resolve("any", WidgetKind::Button, "Delete Save"),
            Some(SfxCueId::ModalCancel)
        );
    }

    #[test]
    fn label_heuristic_classifies_save_as_modal_confirm() {
        let mut policy = SfxPolicy::default();
        assert_eq!(
            policy.resolve("any", WidgetKind::Button, "Save"),
            Some(SfxCueId::ModalConfirm)
        );
        assert_eq!(
            policy.resolve("any", WidgetKind::Button, "OK"),
            Some(SfxCueId::ModalConfirm)
        );
    }

    #[test]
    fn master_disabled_returns_none() {
        let mut policy = SfxPolicy {
            master_enabled: false,
            ..SfxPolicy::default()
        };
        assert_eq!(policy.resolve("any", WidgetKind::Button, "Foo"), None);
    }

    #[test]
    fn per_panel_override_takes_precedence_over_kind_default() {
        let mut policy = SfxPolicy {
            panel_overrides: vec![(
                "fleets".to_string(),
                KindMap {
                    button: Some(SfxCueId::ModalConfirm),
                    ..Default::default()
                },
            )],
            ..SfxPolicy::default()
        };
        assert_eq!(
            policy.resolve("fleets", WidgetKind::Button, "Foo"),
            Some(SfxCueId::ModalConfirm)
        );
        // Fallthrough on a different panel.
        assert_eq!(
            policy.resolve("research", WidgetKind::Button, "Foo"),
            Some(SfxCueId::ButtonClick)
        );
    }
}
