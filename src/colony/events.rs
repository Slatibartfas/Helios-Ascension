//! Construction Bevy `Message` types — emitted by the colony / shipbuilding
//! sim systems and consumed by the notifications surface (PR-C, GRA-137) and
//! the dossier UI.
//!
//! PR-C (GRA-137) adds `ConstructionEvent` to wire the sim layer into the
//! notifications plugin's `EventBridge` system set. The two variants mirror
//! the issue spec:
//!
//! - `Completed { colony, building }` — a colony building finished
//!   construction. Fired from `advance_construction` when a
//!   `ConstructionProject`'s progress reaches `required`.
//! - `ShipCompleted { hull }` — a ship hull finished build-out. There is no
//!   sim system firing this today (the shipbuilding workspace is UI-only).
//!   The variant is defined for forward compatibility and to give the
//!   `bridge_construction_events` test a target; the actual fire site will
//!   land when the shipbuilding sim layer is wired in (a follow-up ticket).
//! - `OutpostEstablished { colony, body }` — GRA-787 closes the producer gap
//!   that GRA-786's design comment called out: the architecture expected
//!   `ConstructionEvent::Completed { building: BuildingType::Outpost }`, but
//!   no such `BuildingType` variant exists — establishing an outpost is a
//!   separate colony-tier promotion that bypasses the normal
//!   building-completion flow. Rather than widen `BuildingType` (which would
//!   touch `all()`, `display_name()`, `description()`, RON validators, and
//!   downstream consumers), this variant surfaces the same milestone as a
//!   dedicated event. The milestone consumer in `crate::survey::milestones`
//!   flips `EarlyGameMilestones::outpost_established` from this variant; the
//!   existing notification bridge in
//!   `crate::ui::notifications::systems::event_bridge` intentionally does not
//!   handle it (no notification category was added — that is a separate
//!   design surface owned by LGD).
//!
//! These messages are transient — they live in a Bevy `Messages<T>` buffer
//! and are dropped between game sessions.

use bevy::prelude::*;

use crate::colony::types::BuildingType;

/// All state transitions the colony construction system emits.
#[derive(Message, Debug, Clone)]
pub enum ConstructionEvent {
    /// A colony building finished construction. The `building` is the
    /// `BuildingType` that was completed; `colony` is the colony entity
    /// that now owns it.
    Completed {
        colony: Entity,
        building: BuildingType,
    },
    /// A ship hull finished build-out. The `hull` is the ship-hull id
    /// (matches `ShipHullDefinition.id`). No sim system fires this today;
    /// reserved for when the shipbuilding workspace migrates to a sim
    /// system.
    ShipCompleted { hull: String },
    /// A new outpost colony was established on a body. Fired from
    /// `process_construction_actions` after a successful outpost promotion.
    /// See the module-level doc for the producer-gap rationale.
    OutpostEstablished { colony: Entity, body: Entity },
}
