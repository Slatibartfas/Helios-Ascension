//! Sound-effects plugin (`SfxPlugin`).
//!
//! One-shot UI / event sting playback layer. Mirrors the
//! lifecycle pattern of [`crate::plugins::music`] (MP3
//! playlist + `AudioPlayer` + `PlaybackSettings::DESPAWN`)
//! but is **event-driven** (no looping, no auto-advance)
//! and **data-driven** (cues are loaded from
//! `assets/data/sfx_manifest.ron`).
//!
//! ## Architecture
//!
//! ```text
//!   assets/data/sfx_manifest.ron   ─┐
//!                                    │  Startup
//!                                    ▼
//!        SfxRegistry (cue + asset-handle map)
//!                                    │
//!   UI clicks / notification toast  │  Update
//!   ──── MessageWriter<SfxEvent>──►│
//!                                    ▼
//!   play_sfx_system: filters by cooldown,
//!   reads `SfxBus` for per-category volume,
//!   spawns one AudioPlayer per cue
//!                                    │
//!                                    ▼
//!   bevy_audio sink plays + auto-despawns
//! ```
//!
//! ## Per-category volume
//!
//! The `SfxBus` resource mirrors the notification layer's
//! [`NotificationCategoryId`] taxonomy. Each category has a
//! volume multiplier that the live `play_sfx_system` reads
//! before spawning a player. The master volume is sourced
//! from [`PersistentSettings::sfx_volume`], which has been
//! persisted to disk but had no audio consumer prior to
//! this PR — see `src/ui/launch/userdata.rs`.
//!
//! ## Moddability
//!
//! `assets/data/sfx_manifest.ron` is the modder surface.
//! Drop a `.wav` into `assets/audio/sfx/` with the same
//! `file` name and the new audio plays on next launch.
//! Add a new cue by appending to the manifest *and* adding
//! the matching variant to [`SfxCueId`] — the audit script
//! `scripts/audit_sfx_manifest.py` enforces the
//! correspondence in CI.

use bevy::audio::{AudioPlugin, Volume};
use bevy::prelude::*;
use std::time::Duration;

pub mod bridges;
pub mod bus;
pub mod manifest;
pub mod playback;

#[cfg(test)]
mod tests;

pub use bus::SfxBus;
pub use playback::SfxRegistry;

// `SfxCue` and `SfxManifest` are defined in this module
// (the manifest schema lives next to the `SfxCueId` +
// `SfxCategory` enums for compile-time locality with
// `SfxCueId::ALL`). They are reachable via
// `crate::plugins::sfx::SfxCue` and `crate::plugins::sfx::SfxManifest`.

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Sound-effects plugin. Wires the [`SfxRegistry`] from disk, the
/// [`SfxBus`] for per-category volume routing, the `play_sfx_system`
/// that consumes [`SfxEvent`]s, and the two bridges that emit
/// them (UI clicks + notification toasts).
///
/// Mirrors [`crate::plugins::music::MusicPlugin::build`] but does
/// **not** own `AudioPlugin` — that lives in Bevy's `DefaultPlugins`
/// and [`MusicPlugin`] already verifies it's installed. We piggy-back
/// on the same plugin instance so the music and SFX share the audio
/// thread / device.
pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        // Same defensive pattern as MusicPlugin — if for some reason
        // a custom DefaultPlugins config drops AudioPlugin (none
        // today; this is future-proofing), re-add it so the SFX
        // backend has somewhere to sink into.
        if !app.is_plugin_added::<AudioPlugin>() {
            app.add_plugins(AudioPlugin::default());
        }

        app.init_resource::<SfxRegistry>()
            .init_resource::<SfxBus>()
            .add_message::<SfxEvent>()
            .add_systems(Startup, (manifest::load_sfx_manifest,))
            .add_systems(
                Update,
                (
                    bus::sync_sfx_bus_volume,
                    bridges::notifications::notification_sfx_bridge,
                    bridges::ui::ui_sfx_bridge,
                    playback::play_sfx_system,
                )
                    // Run order:
                    //   1. Sync volumes from settings (cheap, runs every frame).
                    //   2. Drain notification events → SfxEvent (one chime per
                    //      *coalesced* toast; the cooldown in SfxBus makes this
                    //      safe even if coalesce produced 10 toasts in a frame).
                    //   3. Drain UI writers (cheap, in-line at the call sites).
                    //   4. play_sfx_system spawns AudioPlayers.
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// Cue IDs
// ---------------------------------------------------------------------------

/// Stable, compile-time-safe identifier for every cue.
///
/// **Adding a cue**:
/// 1. Add the variant here.
/// 2. Add the matching entry in `assets/data/sfx_manifest.ron`.
/// 3. Add the prompt in `assets/data/sfx_prompts.ron`.
/// 4. Run `scripts/generate_sfx.py` to produce the WAV.
/// 5. Run `scripts/audit_sfx_manifest.py` to verify the link.
///
/// `scripts/audit_sfx_manifest.py` enforces step 1↔2 in CI; if
/// you skip step 1 the build still compiles but the new manifest
/// entry is unreachable at runtime.
///
/// Phase 1 surface: 13 UI + 1 notification. Later PRs add
/// construction / research / shipbuilding / fleet / survey /
/// economy / colony / camera / time-control / launch / persistence
/// variants here as the corresponding manifest entries land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxCueId {
    // ── UI chrome ─────────────────────────────────────────────────
    ButtonClick,
    TabSwitch,
    PanelOpen,
    PanelClose,
    SliderTick,
    DropdownOpen,
    RowSelect,
    DragDrop,
    ModalConfirm,
    ModalCancel,
    ChipToggle,
    ModeToggle,
    // ── Notifications ─────────────────────────────────────────────
    /// Universal chime played once per coalesced toast.
    /// See [`bridges::notifications::notification_sfx_bridge`].
    NotificationChime,
}

impl SfxCueId {
    /// String id used in `sfx_manifest.ron` (e.g. `"ui.button_click"`).
    /// Returned as `None` if the id is unknown — callers should log
    /// and skip rather than panic on manifest / code drift.
    pub fn as_str_id(&self) -> &'static str {
        match self {
            Self::ButtonClick => "ui.button_click",
            Self::TabSwitch => "ui.tab_switch",
            Self::PanelOpen => "ui.panel_open",
            Self::PanelClose => "ui.panel_close",
            Self::SliderTick => "ui.slider_tick",
            Self::DropdownOpen => "ui.dropdown_open",
            Self::RowSelect => "ui.row_select",
            Self::DragDrop => "ui.drag_drop",
            Self::ModalConfirm => "ui.modal_confirm",
            Self::ModalCancel => "ui.modal_cancel",
            Self::ChipToggle => "ui.chip_toggle",
            Self::ModeToggle => "ui.mode_toggle",
            Self::NotificationChime => "notifications.chime",
        }
    }

    /// Reverse of [`Self::as_str_id`]. Returns `None` for unknown
    /// ids (the manifest loader logs + skips them rather than
    /// treating this as a hard error, since modders may add ids
    /// in a later release).
    pub fn from_str_id(s: &str) -> Option<Self> {
        Some(match s {
            "ui.button_click" => Self::ButtonClick,
            "ui.tab_switch" => Self::TabSwitch,
            "ui.panel_open" => Self::PanelOpen,
            "ui.panel_close" => Self::PanelClose,
            "ui.slider_tick" => Self::SliderTick,
            "ui.dropdown_open" => Self::DropdownOpen,
            "ui.row_select" => Self::RowSelect,
            "ui.drag_drop" => Self::DragDrop,
            "ui.modal_confirm" => Self::ModalConfirm,
            "ui.modal_cancel" => Self::ModalCancel,
            "ui.chip_toggle" => Self::ChipToggle,
            "ui.mode_toggle" => Self::ModeToggle,
            "notifications.chime" => Self::NotificationChime,
            _ => return None,
        })
    }

    /// Every variant — used by the audit script and by tests to
    /// assert "every Rust variant has a manifest entry".
    pub const ALL: &'static [SfxCueId] = &[
        Self::ButtonClick,
        Self::TabSwitch,
        Self::PanelOpen,
        Self::PanelClose,
        Self::SliderTick,
        Self::DropdownOpen,
        Self::RowSelect,
        Self::DragDrop,
        Self::ModalConfirm,
        Self::ModalCancel,
        Self::ChipToggle,
        Self::ModeToggle,
        Self::NotificationChime,
    ];
}

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

/// Per-cue category. Mirrors (a superset of) the notification
/// `NotificationCategoryId` taxonomy so the same per-category
/// mute / volume logic can apply. Phase 1 only wires `Ui` and
/// `Notifications`; the rest are declared now so the manifest
/// schema doesn't change when later PRs add their cues.
///
/// Mapping to notification categories (when applicable) is
/// done in [`SfxBus::notification_category_for`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, serde::Serialize, serde::Deserialize,
)]
pub enum SfxCategory {
    Ui,
    Construction,
    Research,
    Engineering,
    Shipbuilding,
    Fleets,
    Notifications,
    Economy,
    Colony,
    Survey,
    Camera,
    TimeControl,
    Launch,
    Persistence,
    Personnel,
}

impl SfxCategory {
    /// String form (used in the manifest and in logs).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ui => "Ui",
            Self::Construction => "Construction",
            Self::Research => "Research",
            Self::Engineering => "Engineering",
            Self::Shipbuilding => "Shipbuilding",
            Self::Fleets => "Fleets",
            Self::Notifications => "Notifications",
            Self::Economy => "Economy",
            Self::Colony => "Colony",
            Self::Survey => "Survey",
            Self::Camera => "Camera",
            Self::TimeControl => "TimeControl",
            Self::Launch => "Launch",
            Self::Persistence => "Persistence",
            Self::Personnel => "Personnel",
        }
    }

    /// Map an SFX category to its corresponding notification
    /// category id (for sound-on lookups), or `None` for
    /// categories that don't have a notification analogue
    /// (e.g. `Ui`, `Camera`, `TimeControl`, `Launch`,
    /// `Persistence`).
    pub fn notification_category_for(&self) -> Option<&'static str> {
        Some(match self {
            Self::Construction => "construction.complete",
            Self::Research => "research.tech_unlocked",
            Self::Engineering => "research.tech_unlocked",
            Self::Shipbuilding => "construction.complete",
            Self::Fleets => "construction.complete",
            Self::Notifications => return None, // handled specially
            Self::Economy => "economy.stockpile_critical",
            Self::Colony => "construction.complete",
            Self::Survey => "survey.mission_complete",
            Self::Camera => return None,
            Self::TimeControl => return None,
            Self::Launch => return None,
            Self::Persistence => return None,
            Self::Personnel => "construction.complete",
            Self::Ui => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Cue metadata (lives in the manifest)
// ---------------------------------------------------------------------------

/// One cue as declared in `assets/data/sfx_manifest.ron`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SfxCue {
    /// Stable id (e.g. `"ui.button_click"`). Matches one of the
    /// `SfxCueId::ALL` strings via [`SfxCueId::from_str_id`].
    pub id: String,
    /// Filename relative to `assets/audio/sfx/`. Must end in
    /// `.wav` (Phase 1); later PRs may add `.ogg`/`.mp3` if the
    /// corresponding Cargo features are enabled.
    pub file: String,
    pub category: SfxCategory,
    /// Linear volume multiplier on top of the player's SFX master
    /// and the per-category mute gate. Range 0.0..=1.0; values
    /// outside this range are clamped at playback time.
    pub default_volume: f32,
    /// Minimum interval between two successive plays of this cue.
    /// Prevents saturation on high-frequency inputs (slider drag,
    /// rapid tab switches). `0` disables the cooldown.
    pub cooldown_ms: u32,
    /// Natural-language prompt used to generate the cue. Stored
    /// alongside the manifest entry so modders can see the design
    /// intent and re-generate the WAV if needed.
    pub prompt: String,
}

/// Top-level manifest structure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SfxManifest {
    pub cues: Vec<SfxCue>,
}

// ---------------------------------------------------------------------------
// Event bus
// ---------------------------------------------------------------------------

/// Bevy message — every cue play request is one of these.
///
/// **Producers** (call `writer.write(SfxEvent(id))`):
/// - [`bridges::ui::ui_sfx_bridge`] — drains the UI message
///   sub-bus each frame.
/// - [`bridges::notifications::notification_sfx_bridge`] —
///   queries `Added<ActiveNotification>` and emits one chime per
///   coalesced toast.
///
/// **Consumer**: [`playback::play_sfx_system`] — looks up the
/// cue in [`SfxRegistry`], respects the per-cue cooldown, reads
/// [`SfxBus`] for the per-category volume, then spawns a
/// `bevy_audio::AudioPlayer` with `PlaybackMode::Despawn`.
#[derive(Debug, Clone, Message)]
pub struct SfxEvent(pub SfxCueId);

// ---------------------------------------------------------------------------
// Shared helpers (re-exported for tests + bridges)
// ---------------------------------------------------------------------------

/// Convert a `cooldown_ms` into a Bevy `Duration` for clock math.
pub(crate) fn cooldown_duration(c: &SfxCue) -> Duration {
    Duration::from_millis(c.cooldown_ms as u64)
}

/// Read the per-cue volume multiplier from a cue, clamping to
/// `[0.0, 1.0]`. Defends against out-of-range entries authored
/// in the manifest (the loader doesn't reject them; the audit
/// script warns).
pub(crate) fn clamped_default_volume(c: &SfxCue) -> f32 {
    c.default_volume.clamp(0.0, 1.0)
}

/// Convenience for tests + the playback system: build a Bevy
/// `Volume::Linear` from a float, clamping to `[0.0, 1.0]`.
pub(crate) fn linear_volume(v: f32) -> Volume {
    Volume::Linear(v.clamp(0.0, 1.0))
}

// ---------------------------------------------------------------------------
// Internal helpers (re-exported for tests)
// ---------------------------------------------------------------------------

/// Build the asset path (`audio/sfx/<file>`) for a cue.
pub(crate) fn asset_path_for(cue: &SfxCue) -> String {
    format!("audio/sfx/{}", cue.file)
}
