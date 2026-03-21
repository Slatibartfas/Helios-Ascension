use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod refit;
pub mod slipway;
pub mod systems;
pub mod types;

pub use components::{
    LaunchCapacityState, OrbitalStation, PendingShipbuildingActions,
    QueueShipConstructionAction, ShipConstructionProject, ShipConstructionState,
    ShipDesignDraft, ShipModuleSelection,
};
pub use data::{
    HullSlotDefinition, ShipDesignLibrary, ShipDesignSummary, ShipHullDefinition,
    ShipModuleDefinition, ShipbuildingData, load_shipbuilding_data,
};
pub use slipway::{ShipyardFacility, Slipway};
pub use types::{ConstructionMode, HullSizeTier, ShipDesignTemplate, ShipModuleCategory};

/// Plugin that registers the modular shipbuilding subsystem.
pub struct ShipbuildingPlugin;

impl Plugin for ShipbuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingShipbuildingActions>()
            .init_resource::<LaunchCapacityState>()
            .init_resource::<ShipDesignLibrary>()
            .add_systems(Startup, load_shipbuilding_data)
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