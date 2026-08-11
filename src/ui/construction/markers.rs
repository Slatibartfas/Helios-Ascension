//! Component markers used by the construction UI systems.
//!
//! These are pure `#[derive(Component)]` markers used to find
//! specific entities (cards, chips, CTAs, scrollbars, etc.) in
//! per-frame systems. They have no logic — see the system modules
//! for the systems that read them.

use bevy::prelude::*;

// Marker component for the canary root container.
#[derive(Component)]
pub struct ConstructionRoot;

// Marker component for the AppBar title text.
#[derive(Component)]
pub struct ConstructionTitle;

// Marker component for the AppBar subtitle text.
#[derive(Component)]
pub struct ConstructionSubtitle;

// Marker component for each build card. Holds the building name.
#[derive(Component)]
pub struct ConstructionCard {
    pub name: String,
}

// v0.5.2 (2026-08-06): marker for **Build-tab-only** cards.
//
// `spawn_card` (the shared chrome builder) attaches `ConstructionCard`
// to every card — Build, Buildings, AND Mining. `refresh_card_grid`
// used to despawn `Query<Entity, With<ConstructionCard>>` — i.e. EVERY
// card in the world — whenever `ConstructionUiState` changed (chip
// click, tab switch, mining group collapse). That silently wiped the
// Mining + Buildings cards, and their per-tab caches (the mining
// fingerprint gate + the buildings `spawned_cards` map) had no way to
// recover: the Buildings cache still believed the cards existed, so it
// never re-spawned them (the "Buildings tab shows the header but no
// cards" bug), and the Mining tab churned a full 49-card teardown +
// rebuild on every click (the "Build button flickers" bug).
//
// `refresh_card_grid` now despawns ONLY `With<BuildCard>` entities —
// the cards IT spawned — and inserts `BuildCard` on each. Mining and
// Buildings cards are untouched.
#[derive(Component)]
pub struct BuildCard;

// Marker component for the Queue CTA. Carries the `BuildingType` this
// card represents, so the click handler knows which building to enqueue.
#[derive(Component)]
pub struct ConstructionCta {
    pub building_type: crate::colony::types::BuildingType,
}

// Marker component for Queue CTAs that should be **disabled** (visible
// but inactive). The player can't afford N copies of the building at
// the current multiplier. The `tick_construction_cta_disabled` system
// inserts this marker after each refresh, and the click handler skips
// the push when the marker is present.
#[derive(Component)]
pub struct ConstructionCtaDisabled;

// v0.5.2 (build menu fix): marker for Queue CTAs that are
// **permanently** disabled because the building isn't allowed on
// the current body (e.g. an Iron Mine on a gas giant). Distinct
// from `ConstructionCtaDisabled` so the per-frame
// `tick_construction_cta_disabled` system — which only reasons
// about resources / power — doesn't silently re-enable the CTA
// when the player happens to have the resources on hand. The
// hover system treats this marker identically to
// `ConstructionCtaDisabled` (no hover scale, no colour change);
// the affordability system ignores it (the spawn-time decision
// is permanent until the body changes).
#[derive(Component)]
pub struct ConstructionCtaBodyBlocked;

// Marker component for the AppBar "OPEN QUEUE" toggle chip. The
// `tick_open_queue_chip_click` system reads this marker to know
// which chip's `Interaction::Pressed` should toggle `QueuePanelState`.
#[derive(Component)]
pub struct OpenQueueChip;

// Marker component for the "Active Colony" picker. The
// `tick_colony_picker_click` system reads this marker to toggle
// `ColonyDropdownState::open` when the player clicks the picker.
// Distinct from `OpenQueueChip` so the click handlers can dispatch
// independently.
#[derive(Component)]
pub struct ColonyPicker;

// Marker component for the floating Active Colony dropdown menu
// (the list of colonies that appears below the picker when it's
// clicked). The `tick_colony_dropdown_visibility` system shows /
// hides this container based on `ColonyDropdownState::open`.
#[derive(Component)]
pub struct ColonyDropdownMenu;

// Marker component for a single option row inside the colony dropdown
// menu. Carries the `Entity` of the `Colony` it represents so the
// `tick_colony_option_click` system can update
// `ConstructionUiState::selected_colony` when an option is clicked.
#[derive(Component)]
pub struct ColonyDropdownOption {
    pub colony_entity: bevy::ecs::entity::Entity,
}

// Marker component on the colony value text inside the picker. The
// `update_colony_picker_text` system writes the active colony's name
// here every frame so the label always reflects the current selection.
#[derive(Component)]
pub struct ColonyPickerText;

// Marker component on each row of the colony dropdown menu that holds
// the colony name text. The `refresh_colony_dropdown` system uses
// these to know which rows to keep vs. despawn when the list of
// colonies changes.
#[derive(Component)]
pub struct ColonyDropdownOptionText;

// Marker component for the marquee track that wraps an overflowing
// build-card subtitle. The track holds two copies of the subtitle
// back-to-back (no gap — a gap would create a visible "blank" beat
// when copy A scrolls out and copy B scrolls in) and is animated via
// `UiTransform.translation` by `tick_subtitle_marquee`.
//
// `card`, `text_node`, and `clip_container` are pre-resolved entity
// handles stored at spawn time so the tick system can do direct
// `Query::get` lookups instead of walking `Parent` / `Children`
// chains every frame. All three are required; missing entities
// (the card was despawned mid-tick) just keep the marquee dormant
// so the engine can clean it up.
//
// `text_width` and `container_width` are the most recent
// `ComputedNode`-measured values (pixels) used to decide whether
// the description actually overflows. When `text_width <=
// container_width` the marquee is dormant and the track sits at
// translation `(0, 0)` — the text fits naturally and we leave it
// alone.
//
// `phase` is the current **pixel offset** of the track (always
// non-negative, wrapped modulo `text_width`). At `phase = 0`
// copy A sits at the clip container's left edge; at
// `phase = text_width` copy B sits exactly where copy A
// started — the seamless loop. `tick_subtitle_marquee`
// integrates `phase += dt * PIXELS_PER_SECOND` each tick while
// the description overflows, so the track drifts leftward at
// a constant rate that doesn't depend on card size, hover
// Phase 10: SubtitleMarquee is now an alias for `widgets::Marquee`.
// The legacy `card` field (read by the old `tick_subtitle_marquee`
// to scope per-card drift integration) is gone — the generic
// `widgets::tick_marquee` always animates regardless of card hover,
// matching the previous construction behaviour. The alias keeps
// `use ...::SubtitleMarquee` call sites compiling.
pub use crate::ui::widgets::Marquee as SubtitleMarquee;

// Marker component for the QueuePanel root. The
// `tick_queue_panel_visibility` system hides all but the active
// state — there's only one QueuePanel so this is a singleton query.
#[derive(Component)]
pub struct QueuePanelRoot;

// Marker component for a single row in the queue panel. The
// `update_queue_panel` system spawns one of these per
// `ConstructionProject` for the selected colony, and removes them
// when the project is gone (cancel / complete). The mapping
// `project_entity -> QueueRowEntity` lives in a `Local` storage
// inside the system.
#[derive(Component)]
pub struct QueuePanelRow {
    pub project_entity: Entity,
}

// Marker component for the cancel button on a queue row. Click handler
// pushes the project entity to `PendingConstructionActions::cancel_construction`.
#[derive(Component)]
pub struct QueuePanelRowCancel {
    pub project_entity: Entity,
}

// Marker component for the QueuePanel close button. The click handler
// toggles `QueuePanelState::open` to `false`.
#[derive(Component)]
pub struct QueuePanelClose;

// Marker component for the card grid container. The refresh system
// queries for this to find the grid and re-parent new cards.
#[derive(Component)]
pub struct CardGrid;

// Marker for the always-visible vertical scrollbar track. The
// track pins itself to the right edge of whichever scrollable
// body the construction menu exposes (Build tab card grid, Mining
// tab body, etc.). The `target` field tells
// `tick_construction_scrollbar` which entity's
// `ScrollPosition` + `ComputedNode::content_size` drives the
// thumb's height + Y offset. The `tab` field tells
// `tick_construction_body_visibility` which tab this scrollbar
// belongs to so it can show/hide alongside the body. This is the
// v0.5.2 PR-A.8 generalisation of the original Build-tab-only
// `CardGridScrollbarTrack`: the same chrome now appears on every
// construction tab whose content can overflow vertically.
// Phase 5B (2026-08-10): ConstructionScrollbarTrack is now an alias
// for `widgets::ScrollbarTrack`. The legacy `tab` field (used to hide
// non-active tracks via a separate visibility system) is gone — the
// per-track `widgets::ScrollbarMetrics` Component scopes each track
// independently, and `tick_construction_body_visibility` already hides
// the body root, so the track follows naturally via Bevy's inherited
// Visibility. The alias keeps legacy `use ...::ConstructionScrollbarTrack`
// call sites compiling during the migration.
pub use crate::ui::widgets::ScrollbarTrack as ConstructionScrollbarTrack;

// Phase 5B: thumb alias for `widgets::ScrollbarThumb`.
pub use crate::ui::widgets::ScrollbarThumb as ConstructionScrollbarThumb;

// Marker on the card_cta_label text entity so the per-frame
// dim pass can identify it without walking the name. Spawned in
// `spawn_card` alongside the `Text` and `TextFont` components.
#[derive(Component)]
pub struct ConstructionCtaLabelMarker;

// Per-chip data for the cost-row hover tooltip. Carried by
// every `ResourceCostChip` so the observer handlers can look up
// the resource name + amount + category tint via the picked
// entity id and write them into the tooltip's text node.
//
// `name` is the display string (`"Iron"`, `"Water"`, `"He-3"`,
// etc.) — not the raw RON name. `amount` is the formatted
// `kg / t / Mt / Gt / Tt` string produced by `format_mining_reserve`.
// `category` is the chip's category colour (Construction /
// Volatiles / Fissile / etc.) so the tooltip can match the
// chip's tint. `card` is the host card's entity id — the
// observer uses it to find the right tooltip among the many
// (one per visible card).
#[derive(Component, Clone)]
pub struct ResourceCostChip {
    pub name: String,
    pub amount: String,
    pub category: Color,
    pub card: Entity,
}

// Carries the tooltip payload for one Power chip. Attached
// to the chip entity at spawn time; the observer reads it on
// `Pointer<Over>` and writes the lines + tone into
// [`crate::ui::widgets::TooltipRequest`]. The `card` field is
// the host card's entity id — kept for future per-card context
// (e.g. showing the batch's combined cost in the tooltip).
#[derive(Component, Clone)]
pub struct PowerChip {
    pub tooltip_lines: Vec<String>,
    pub tone: Color,
    pub card: Entity,
}

// Marker on a single mine / AutoMine card's outer container.
#[derive(Component)]
pub struct MiningCard {
    pub building_type: crate::colony::types::BuildingType,
}

// Marker on the Mining body's content container.
#[derive(Component)]
pub struct MiningContent;

// Marker on a per-group header row (chevron + label + count).
#[derive(Component)]
pub struct MiningGroupHeader {
    pub group_id: super::state::MiningGroupId,
}

// Marker on a per-group body container (the row of cards). The
// group's visibility is driven by `tick_mining_group_visibility`
// toggling this entity's `Display`.
#[derive(Component)]
pub struct MiningGroupBody {
    pub group_id: super::state::MiningGroupId,
}

// Which build multiplier the Demolish button should use when it
// opens the confirmation dialog. The Mining tab uses its own
// `mining_build_multiplier` (independent of the Build tab's
// `build_multiplier` because the chip sets can diverge), while
// the Buildings tab uses the Build tab's `build_multiplier` so
// the player's x1/x5/x25 chip choice carries straight through to
// the demolish button. The dialog reads the multiplier via
// `DemolishConfirmState::count` (clamped to the live count).
//
// v0.5.2 (Buildings tab as a card list): the Demolish button
// was originally a Mining-tab-only feature. Extending it to the
// Buildings tab means the same handler (`tick_demolish_click`)
// needs to know which chip row governs the multiplier; the
// `DemolishButton` marker carries a `multiplier_source` field
// that the click handler reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemolishMultiplierSource {
    // Use `ui_state.mining_build_multiplier`. Mining tab cards.
    Mining,
    // Use `ui_state.build_multiplier`. Buildings tab cards.
    Build,
}

// Marker on a Demolish button. The button removes `mining_edits`
// (negative delta) inventory from the active colony via
// `PendingConstructionActions::mining_edits`. The handler
// `tick_demolish_click` does the actual work.
//
// Originally a Mining-tab-only feature (`MiningDemolishButton`),
// v0.5.2 (Buildings tab as a card list) extends the same button
// to the Buildings tab so the player can demolish existing
// buildings with the same UI. The `multiplier_source` field tells
// the click handler which chip-row value to apply (Mining vs
// Build) — they can diverge.
#[derive(Component)]
pub struct DemolishButton {
    pub building_type: crate::colony::types::BuildingType,
    pub multiplier_source: DemolishMultiplierSource,
}

// v0.5.2 (2026-08-06): marker on the Demolish button's label text.
// `update_demolish_button_labels` rewrites it every frame from the
// live `build_multiplier` — the old code spawned the label once at
// card-spawn time, so a Buildings-tab card (spawn-once-update-many)
// kept "Demolish -1" forever even after the player picked ×25.
#[derive(Component)]
pub struct DemolishButtonLabel;

// Marker added when the Demolish button should be disabled (no
// buildings to remove — `count == 0`). Mirrors
// `ConstructionCtaDisabled` for the Queue button: the click
// handler skips pushing when the marker is present, and the
// spawn code drops the marker at spawn time when the count is
// non-zero.
//
// v0.5.2 (Buildings tab as a card list): the Buildings tab
// cards also use this marker so the same hover / click effect
// system (`tick_demolish_hover`, `tick_demolish_disabled`) can
// service both tab families without per-tab branches.
#[derive(Component)]
pub struct DemolishDisabled;

// ── Demolish confirmation modal (v0.5.2 PR-A.7) ──────────────────────

// Marker on the centered Demolish confirmation modal root.
// Visibility is driven by `tick_demolish_dialog_visibility` based on
// `DemolishConfirmState::open`. Parented to the Construction menu
// root, fills the entire menu area with a semi-transparent dark
// backdrop and centers the content card. The Construction menu
// root sits at `top: 126.0`; absolutely-positioned children are
// measured against the *parent's content-area* (so the backdrop
// fills the area below the global chrome, not the window).
#[derive(Component)]
pub struct DemolishConfirmDialog;

// Marker on the dialog title text. Updated by
// `update_demolish_dialog_text` to read e.g. "Demolish 5 Iron
// Mines?" — the count is clamped to the live colony count.
#[derive(Component)]
pub struct DemolishConfirmTitle;

// Marker on the dialog subtitle text. Updated to read e.g.
// "You currently have 5 on Earth." — the live count + colony
// name. Lets the player double-check before confirming.
#[derive(Component)]
pub struct DemolishConfirmSubtitle;

// Marker on the Yes button. `tick_demolish_confirm_yes_click`
// applies the `mining_edits` entry and closes the dialog.
#[derive(Component)]
pub struct DemolishConfirmYes;

// Marker on the No button. `tick_demolish_confirm_no_click`
// closes the dialog without applying the action. The backdrop
// also clicks-through to No via a `Pointer<Click>` observer.
#[derive(Component)]
pub struct DemolishConfirmNo;

// Marker component for the ETA text on a queue row. The
// `update_queue_row_eta` system uses this to find the text node by
// project entity without iterating every `Text` node in the world.
#[derive(Component)]
pub struct QueuePanelRowEta {
    pub project_entity: Entity,
}

// Phase 10: alias for `widgets::ProgressFill`. The construction-side
// queue row progress bar now goes through the generic `tick_progress_fill`
// primitive. The old `QueuePanelRowFill { project_entity }` field is gone
// because the progress percentage is now stored in the
// `widgets::ProgressFill(f32)` Component directly. The spawn site
// writes the percentage each frame as the project advances.
pub use crate::ui::widgets::ProgressFill as QueuePanelRowFill;

// Marker component for the `queue_value` text in the AppBar so the
// `update_queue_summary` system can find it without iterating every
// Text node.
#[derive(Component)]
pub struct QueuePanelSummaryText;

// Marker component for the QueuePanel body container (the scrollable
// column where rows are spawned). The `update_queue_panel` system
// queries for this to find the parent for new rows.
#[derive(Component)]
pub struct QueuePanelBody;

// Chip-kind enum moved to `crate::ui::widgets::ChipGroup` (Phase 2).
// The construction-specific `BuildFilter` filter chip is dropped —
// the tick systems treated it as always-inactive, so the
// construction click handler can route it straight to the
// `BuildFilter` resource without involving the chip tick pipeline.
