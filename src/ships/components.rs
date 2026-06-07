//! Per-ship freighter template components (GRA-40).
//!
//! A freighter entity carries:
//! * a [`ShipTemplateRef`] pointing at one entry in the
//!   [`FreighterTemplateRegistry`](super::templates::FreighterTemplateRegistry)
//!   (the registry is keyed by template id, e.g. `"light_freighter"`);
//! * one [`ShipSlot`] per cargo slot in the template, recording the
//!   currently installed module id and the slot's current upgrade tier.
//!
//! Cargo capacity is computed at query time by summing the `cargo_capacity_t`
//! attribute of each `ShipSlot.installed_module` — see
//! [`freighter_cargo_capacity_t`](super::templates::freighter_cargo_capacity_t).
//!
//! Both components are `Serialize + Deserialize` so save files persist the
//! template assignment.  New ships get the components assigned at construction
//! time (see `shipbuilding::systems::process_ship_launches_and_completions`);
//! existing ships from pre-GRA-40 saves get them assigned at startup by
//! [`migration::migrate_legacy_freighters`](super::migration::migrate_legacy_freighters).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component for entities that are freighters and should be queried
/// for freighter-template data.
///
/// Kept as a separate component (rather than reusing the existing
/// `ShipClass::Freighter` enum on `ShipInfo`) so the template system has a
/// cheap, indexable handle for ECS queries that does not require
/// `info.class` introspection.  The construction completion system adds this
/// alongside `ShipInstance` for any freighter; the migration shim does the
/// same for legacy entities.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FreighterTemplateMarker;

/// Points a freighter entity at one entry in the
/// [`FreighterTemplateRegistry`](super::templates::FreighterTemplateRegistry).
///
/// `template_id` is the registry key (matches `FreighterTemplate.id` in
/// `assets/data/freighter_templates.ron`, e.g. `"light_freighter"`).  Storing
/// the id as `String` keeps the component simple and save-format-stable
/// across RON changes; the registry handles the lookup.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct ShipTemplateRef {
    pub template_id: String,
}

impl ShipTemplateRef {
    pub fn new(template_id: impl Into<String>) -> Self {
        Self {
            template_id: template_id.into(),
        }
    }
}

/// Per-slot state on a freighter entity.
///
/// The Coder stores the installed module id (a key into
/// [`ShipbuildingData::modules`](crate::shipbuilding::ShipbuildingData::modules))
/// and the slot's current upgrade tier.  Tier indexing follows the design
/// doc: `upgrade_tier == 0` means default (`default_module` from the
/// template), `upgrade_tier == N > 0` means `upgrade_path[N - 1].module` is
/// installed.  See `docs/design/SHIP_TEMPLATES.md` §"Tier-index mapping".
///
/// `ShipSlot` itself is a value type.  Per-entity state is stored in
/// [`FreighterSlots`], a Component that wraps a `Vec<ShipSlot>` — one entry
/// per cargo slot on the template.  Bevy 0.18 does not support multiple
/// components of the same type on a single entity, so all slots live in
/// this single Component.  The slot's `slot_id` matches a
/// `HullSlotDefinition.slot_id` on the template's base hull (and resolves
/// to the same name in the `FreighterTemplate.cargo_slots` list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipSlot {
    pub slot_id: String,
    pub installed_module: String,
    pub upgrade_tier: u32,
}

impl ShipSlot {
    pub fn new(
        slot_id: impl Into<String>,
        installed_module: impl Into<String>,
        upgrade_tier: u32,
    ) -> Self {
        Self {
            slot_id: slot_id.into(),
            installed_module: installed_module.into(),
            upgrade_tier,
        }
    }
}

/// Component wrapping a freighter entity's per-slot state.  One entry per
/// cargo slot in the entity's [`ShipTemplateRef`] template.  Inserted at
/// construction time (see `shipbuilding::systems::process_ship_launches_and_completions`)
/// and at startup by the migration shim for legacy `ShipClass::Freighter`
/// entities (see [`migration::migrate_legacy_freighters`](super::migration::migrate_legacy_freighters)).
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct FreighterSlots(pub Vec<ShipSlot>);

impl FreighterSlots {
    pub fn new(slots: Vec<ShipSlot>) -> Self {
        Self(slots)
    }

    /// Look up a slot by `slot_id`.  Returns `None` if the entity does not
    /// have a slot for that id (e.g. the slot was removed by a refit).
    pub fn get(&self, slot_id: &str) -> Option<&ShipSlot> {
        self.0.iter().find(|s| s.slot_id == slot_id)
    }
}
