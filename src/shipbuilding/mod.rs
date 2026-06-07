use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod refit;
pub mod slipway;
pub mod systems;
pub mod types;

pub use components::{
    LaunchCapacityState, OrbitalStation, PendingShipbuildingActions, QueueShipConstructionAction,
    ShipConstructionProject, ShipConstructionState, ShipDesignAssignment, ShipDesignDraft,
    ShipModuleSelection,
};
pub use data::{
    load_shipbuilding_data, HullSlotDefinition, ShipDesignLibrary, ShipDesignSummary,
    ShipHullDefinition, ShipModuleDefinition, ShipbuildingData,
};
pub use refit::{QueueRefitAction, RefitProject, RefitType};
pub use slipway::{ShipyardFacility, Slipway};
pub use types::{ConstructionMode, HullSizeTier, ShipDesignTemplate, ShipModuleCategory};

use crate::ships::templates as ship_templates;

/// Plugin that registers the modular shipbuilding subsystem.
pub struct ShipbuildingPlugin;

impl Plugin for ShipbuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingShipbuildingActions>()
            .init_resource::<LaunchCapacityState>()
            .init_resource::<ShipDesignLibrary>()
            .add_systems(Startup, load_shipbuilding_data)
            // GRA-40: freighter template loading must run after hull + module
            // data is loaded (the loader validates against ShipbuildingData),
            // and the migration shim must run after the registry is populated
            // (so it can resolve the light_freighter template).  Chain all
            // three explicitly; the default Startup schedule is otherwise
            // unordered and the 3 would race in parallel (Bevy 0.18).
            .add_systems(
                Startup,
                (
                    load_shipbuilding_data,
                    ship_templates::load_freighter_templates,
                    crate::ships::migration::migrate_legacy_freighters,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    systems::process_pending_shipbuilding_actions,
                    systems::advance_ship_construction,
                    systems::process_ship_launches_and_completions,
                )
                    .chain(),
            );
    }
}
