//! Freighter template system (GRA-40).
//!
//! Refactors the legacy `ShipClass::Freighter` flat enum into a RON-driven
//! template + slot list model, with tech-gated slot upgrades.  See
//! `docs/design/SHIP_TEMPLATES.md` for the design.
//!
//! The module exposes:
//! * [`FreighterTemplateRegistry`] — Bevy `Resource` loaded at startup from
//!   `assets/data/freighter_templates.ron`.  Indexed by template id; also
//!   tracks the LGD-defined `light_freighter` id used for the 1:1 migration
//!   of legacy `ShipClass::Freighter` entities.
//! * [`ShipTemplateRef`], [`FreighterSlots`] (wrapping `Vec<ShipSlot>`) —
//!   per-ship components.  A freighter entity carries one `ShipTemplateRef`
//!   plus one `FreighterSlots` Component holding one `ShipSlot` per
//!   template cargo slot.  Cargo capacity is the sum of installed modules'
//!   attribute `cargo_capacity_t` at query time.
//! * [`freighter_cargo_capacity_t`] / [`best_buildable`] — query helpers
//!   used by logistics (this issue) and the GRA-39 auto-construction AI.

use bevy::prelude::*;

pub mod components;
pub mod migration;
pub mod templates;

pub use components::{FreighterSlots, FreighterTemplateMarker, ShipSlot, ShipTemplateRef};
pub use templates::{
    freighter_cargo_capacity_t, freighter_cargo_capacity_t_for_components,
    freighter_cargo_capacity_t_for_entity, load_freighter_templates, BestBuildableEntry, CargoSlot,
    FreighterTemplate, FreighterTemplateRegistry, UpgradeStep, LEGACY_MIGRATION_TEMPLATE_ID,
};

/// System set used to order template loading relative to other Startup work.
///
/// `Startup` runs every resource init + startup system once when the `App` is
/// built.  We order the freighter template loader before the migration system
/// so the registry is populated before we try to assign templates to legacy
/// entities.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShipTemplatesStartupSet;
