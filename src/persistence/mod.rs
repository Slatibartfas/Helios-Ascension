//! Save / Load plugin — Bevy world-state persistence for Helios Ascension.
//!
//! GRA-314 PR-A. PR-A ships:
//!
//! - [`PersistencePlugin`] — registers [`AppTypeRegistry`] (PR-A safety net,
//!   production plugin registrations in PR-B/C/D add their own component
//!   registrations via `app.register_type::<T>()`).
//! - [`snapshot::snapshot_world`] — serialise the live world to a RON string.
//! - [`restore::restore_world`] — deserialise a RON string into a fresh
//!   [`World`].
//! - [`migrate::run_migrations`] — version-aware forward migrator chain.
//! - [`format_version::FORMAT_VERSION`] — `1` in PR-A.
//!
//! # R2 (Reflection coverage gap)
//!
//! Bevy's `DynamicScene` snapshot walks `AppTypeRegistry` for every
//! `#[reflect(Component)]` and `#[reflect(Resource)]` type. Helios currently
//! registers very few types reflectively — only
//! `src/ui/notifications/components.rs` and a couple of UI state types.
//! **PR-A's roundtrip test therefore exercises reflectively-registered
//! test types only**, not live Helios components.
//!
//! The gap is tracked as a follow-up issue (search for "GRA-XXX add
//! `#[reflect(Component)]` across astronomy/colony/fleet/economy"). Until
//! that lands, calling [`snapshot_world`] on a real Helios world will
//! silently drop every component that hasn't been registered.
//!
//! # R3 (fresh world for restore)
//!
//! [`restore::restore_world`] ALWAYS constructs a fresh [`World`] via the
//! caller-supplied factory. We never reuse the live world for restore —
//! `Entity` IDs in Bevy 0.18 are reused after `World::clear()`, which would
//! cause silent pointer collisions.
//!
//! # R4 (atomic on-disk write)
//!
//! PR-A does NOT touch the disk — the save panel in PR-B will add the
//! `write-to-tmp-then-rename` pattern. PR-A only produces a RON string.
//!
//! # Bevy 0.18 `SceneDeserializer` import gotcha
//!
//! `bevy_scene::serde::SceneDeserializer` only exposes `.deserialize(...)`
//! via the `serde::de::DeserializeSeed` trait — there is no inherent method.
//! Any caller of the restore path MUST `use serde::de::DeserializeSeed;`
//! alongside the `use bevy_scene::serde::SceneDeserializer;`. PR-B and PR-C
//! will both reach into this module; the import lives at module scope to
//! avoid per-call-site duplication.

use bevy::prelude::*;

pub mod format_version;
pub mod migrate;
pub mod restore;
pub mod snapshot;

pub use format_version::{FORMAT_VERSION, MIN_SUPPORTED_VERSION};
pub use migrate::{Body, MigrateError, SchemaKind};
pub use restore::{restore_world, RestoreError, RestoredWorld};
pub use snapshot::{
    snapshot_world, snapshot_world_with_registry, SaveFile, SaveMetadata, SnapshotError,
};

/// Plugin that wires save/load into Bevy.
///
/// PR-A registers [`AppTypeRegistry`] if no other plugin has done so —
/// the snapshot/restore helpers cannot function without it. Production
/// plugins (Persistence itself included in later PRs) register their
/// own component types via `app.register_type::<T>()`.
///
/// GRA-319 expands the registrations to cover the simulation-state
/// Components and persistent Resources across astronomy/colony/economy/
/// fleets/research/survey/shipbuilding/personnel/plugins. Each owning
/// plugin still calls `register_type` for its own types so the
/// coverage survives if the consumer skips Persistence.
pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        // PR-A does not register `Update` systems because save/load runs
        // only on explicit request. The action-queue wiring lives in
        // PR-B (Save Panel UI). PR-A exists so the menu's "Load Game"
        // click can hand a path to a real loader, and so the roundtrip
        // test has a home.
        //
        // Belt-and-braces: ensure AppTypeRegistry exists. Bevy plugins
        // normally register this; some test-only apps do not.
        if !app.world().contains_resource::<AppTypeRegistry>() {
            app.init_resource::<AppTypeRegistry>();
        }

        // GRA-319: cross-plugin reflection coverage. Every
        // simulation-state Component and persistent Resource across the
        // listed plugin dirs is registered here so a `World` snapshot
        // captures real data (DynamicScene::from_world silently drops
        // anything not in AppTypeRegistry). Render-only wrapper types
        // (ProceduralRng, MusicPlaylist, CometTailResources,
        // LinearImageQueue, RingAlphaCombineQueue, EguiPanelBounds's
        // inner rect) are intentionally NOT registered.
        app
            // ── Astronomy ─────────────────────────────────────────────
            .register_type::<crate::astronomy::SpaceCoordinates>()
            .register_type::<crate::astronomy::FloatingOrigin>()
            .register_type::<crate::astronomy::CurrentStarSystem>()
            .register_type::<crate::astronomy::SystemId>()
            .register_type::<crate::astronomy::OrbitCenter>()
            .register_type::<crate::astronomy::KeplerOrbit>()
            .register_type::<crate::astronomy::HyperbolicTrajectory>()
            .register_type::<crate::astronomy::OrbitPath>()
            .register_type::<crate::astronomy::Selected>()
            .register_type::<crate::astronomy::Hovered>()
            .register_type::<crate::astronomy::Destroyed>()
            .register_type::<crate::astronomy::CometTail>()
            .register_type::<crate::astronomy::LocalOrbitAmplification>()
            .register_type::<crate::astronomy::SelectionMarker>()
            .register_type::<crate::astronomy::HoverMarker>()
            .register_type::<crate::astronomy::MarkerOwner>()
            .register_type::<crate::astronomy::MarkerDot>()
            .register_type::<crate::astronomy::LpMarkerInfo>()
            .register_type::<crate::astronomy::OceanType>()
            .register_type::<crate::astronomy::OceanProperties>()
            .register_type::<crate::astronomy::SurfaceTemperature>()
            .register_type::<crate::astronomy::StellarProperties>()
            .register_type::<crate::astronomy::AtmosphericGas>()
            .register_type::<crate::astronomy::AtmosphereComposition>()
            .register_type::<crate::astronomy::RealPlanet>()
            .register_type::<crate::astronomy::NearbyStarsData>()
            .register_type::<crate::astronomy::StarSystemData>()
            .register_type::<crate::astronomy::StarData>()
            .register_type::<crate::astronomy::PlanetData>()
            .register_type::<crate::astronomy::BinaryOrbitData>()
            .register_type::<crate::astronomy::LagrangePointMarkers>()
            .register_type::<crate::astronomy::LastLpClick>()
            .register_type::<crate::astronomy::RingHighlight>()
            // ── Solar System (SystemPopulatorPlugin target) ───────────
            .register_type::<crate::plugins::solar_system::CelestialBody>()
            .register_type::<crate::plugins::solar_system::LogicalParent>()
            .register_type::<crate::plugins::solar_system::Star>()
            .register_type::<crate::plugins::solar_system::Planet>()
            .register_type::<crate::plugins::solar_system::DwarfPlanet>()
            .register_type::<crate::plugins::solar_system::Moon>()
            .register_type::<crate::plugins::solar_system::Asteroid>()
            .register_type::<crate::plugins::solar_system::Comet>()
            .register_type::<crate::plugins::solar_system::GasGiant>()
            .register_type::<crate::plugins::solar_system::Ring>()
            .register_type::<crate::plugins::solar_system::ClickExcluded>()
            .register_type::<crate::plugins::solar_system::AxialTilt>()
            .register_type::<crate::plugins::solar_system::RotationSpeed>()
            .register_type::<crate::plugins::solar_system_data::BodyType>()
            .register_type::<crate::plugins::solar_system_data::AsteroidClass>()
            // ── Economy ────────────────────────────────────────────────
            .register_type::<crate::economy::PowerSourceType>()
            .register_type::<crate::economy::PowerGenerator>()
            .register_type::<crate::economy::StarSystem>()
            .register_type::<crate::economy::Population>()
            .register_type::<crate::economy::SpectralClass>()
            .register_type::<crate::economy::OrbitsBody>()
            .register_type::<crate::economy::ResourceReserve>()
            .register_type::<crate::economy::MineralDeposit>()
            .register_type::<crate::economy::SurveyLevel>()
            .register_type::<crate::economy::PlanetResources>()
            .register_type::<crate::economy::LocalStockpile>()
            .register_type::<crate::economy::RequestPriority>()
            .register_type::<crate::economy::RequestState>()
            .register_type::<crate::economy::ResourceRequest>()
            .register_type::<crate::economy::PendingResourceRequests>()
            .register_type::<crate::economy::MinimumStockpile>()
            .register_type::<crate::economy::MiningOperation>()
            .register_type::<crate::economy::SurveyHistoryStats>()
            .register_type::<crate::economy::SimulationHistorySample>()
            .register_type::<crate::economy::SimulationHistory>()
            .register_type::<crate::economy::AutoBuildNotificationState>()
            .register_type::<crate::economy::AutoFreightNotificationState>()
            .register_type::<crate::economy::ResourceRateTracker>()
            .register_type::<crate::economy::ColonyPowerTotals>()
            .register_type::<crate::economy::GlobalBudget>()
            .register_type::<crate::economy::EnergyGrid>()
            .register_type::<crate::economy::ContextualStockpile>()
            .register_type::<crate::economy::CompanyAIPolicy>()
            .register_type::<crate::economy::CompanyBuildPolicy>()
            .register_type::<crate::economy::ShippingCompany>()
            .register_type::<crate::economy::ShippingCompanies>()
            .register_type::<crate::economy::ResourceType>()
            .register_type::<crate::economy::ResourcePhase>()
            // ── Colony ────────────────────────────────────────────────
            .register_type::<crate::colony::ColonyTier>()
            .register_type::<crate::colony::ColonyDevelopment>()
            .register_type::<crate::colony::Colony>()
            .register_type::<crate::colony::ConstructionProject>()
            .register_type::<crate::colony::PendingConstructionActions>()
            .register_type::<crate::colony::EstablishOutpostRequest>()
            .register_type::<crate::colony::ColonyEnvironmentCosts>()
            .register_type::<crate::colony::AtmosphereKind>()
            .register_type::<crate::colony::BuildingModifierDef>()
            .register_type::<crate::colony::SynergyRule>()
            .register_type::<crate::colony::BuildingDefinition>()
            .register_type::<crate::colony::BuildingsData>()
            .register_type::<crate::colony::DepletionTimeline>()
            .register_type::<crate::colony::ColonySynergies>()
            .register_type::<crate::colony::SynergyState>()
            .register_type::<crate::colony::ConstructionDebugSettings>()
            .register_type::<crate::colony::BuildingEditState>()
            .register_type::<crate::colony::BuildingEditData>()
            .register_type::<crate::colony::BuildingType>()
            .register_type::<crate::colony::BuildingCategory>()
            // ── Fleets ────────────────────────────────────────────────
            .register_type::<crate::fleets::ShipInfo>()
            .register_type::<crate::fleets::ShipInstance>()
            .register_type::<crate::fleets::Fleet>()
            .register_type::<crate::fleets::FleetOrbit>()
            .register_type::<crate::fleets::TransferReferenceFrame>()
            .register_type::<crate::fleets::ActiveManeuver>()
            .register_type::<crate::fleets::PendingFleetActions>()
            .register_type::<crate::fleets::MergeFleetAction>()
            .register_type::<crate::fleets::TransferShipsAction>()
            .register_type::<crate::fleets::AssignShipsAction>()
            .register_type::<crate::fleets::AssignLogisticsRequestAction>()
            .register_type::<crate::fleets::CreateFleetFromShipsAction>()
            .register_type::<crate::fleets::SpawnFleetAction>()
            .register_type::<crate::fleets::StartTransferAction>()
            .register_type::<crate::fleets::AbortToOriginAction>()
            .register_type::<crate::fleets::PlannedTransfer>()
            .register_type::<crate::fleets::HistoricalProbeKind>()
            .register_type::<crate::fleets::HistoricalProbe>()
            .register_type::<crate::fleets::PorkchopConfig>()
            .register_type::<crate::fleets::PorkchopGridDefaults>()
            .register_type::<crate::fleets::PorkchopCategoryOverride>()
            .register_type::<crate::fleets::PorkchopColorStop>()
            .register_type::<crate::fleets::DayOneFleetSpawned>()
            .register_type::<crate::fleets::HistoricalProbesSpawned>()
            .register_type::<crate::fleets::HistoricalProbeScanState>()
            .register_type::<crate::fleets::FleetRole>()
            .register_type::<crate::fleets::ShipClass>()
            .register_type::<crate::fleets::FleetClass>()
            .register_type::<crate::fleets::PropulsionType>()
            // ── Research ──────────────────────────────────────────────
            .register_type::<crate::research::ResearchBuilding>()
            .register_type::<crate::research::EngineeringFacility>()
            .register_type::<crate::research::ResearchTeamCapacity>()
            .register_type::<crate::research::ResearchProject>()
            .register_type::<crate::research::EngineeringProject>()
            .register_type::<crate::research::ResearchTeam>()
            .register_type::<crate::research::ComponentDesign>()
            .register_type::<crate::research::TechModifier>()
            .register_type::<crate::research::ResearchDebugSettings>()
            .register_type::<crate::research::TechTreeEditState>()
            .register_type::<crate::research::ContextMenuState>()
            .register_type::<crate::research::TechEditData>()
            .register_type::<crate::research::PendingResearchActions>()
            .register_type::<crate::research::TechnologiesData>()
            .register_type::<crate::research::ResearchState>()
            .register_type::<crate::research::Technology>()
            .register_type::<crate::research::TechModifierDef>()
            .register_type::<crate::research::ModifierType>()
            .register_type::<crate::research::TechCategory>()
            .register_type::<crate::research::ComponentDefinition>()
            // ── Survey ────────────────────────────────────────────────
            .register_type::<crate::survey::DimensionFidelity>()
            .register_type::<crate::survey::SurveyState>()
            .register_type::<crate::survey::ActiveSurveyMission>()
            .register_type::<crate::survey::AnalysisJob>()
            .register_type::<crate::survey::DetectedAnomaly>()
            .register_type::<crate::survey::FailedMissionRecord>()
            .register_type::<crate::survey::SiteScores>()
            .register_type::<crate::survey::SiteScoreWeights>()
            .register_type::<crate::survey::LandingSite>()
            .register_type::<crate::survey::ExtractionSite>()
            .register_type::<crate::survey::ContinuousSurveyStation>()
            .register_type::<crate::survey::ContinuousStationBonus>()
            .register_type::<crate::survey::ScientistSummary>()
            .register_type::<crate::survey::ReasonTag>()
            .register_type::<crate::survey::SurveyDimensionRegistry>()
            .register_type::<crate::survey::ModderDimensionDef>()
            .register_type::<crate::survey::SurveyInstrumentRegistry>()
            .register_type::<crate::survey::SurveyInstrumentDef>()
            .register_type::<crate::survey::SurveyMissionTemplates>()
            .register_type::<crate::survey::SurveyMissionTemplate>()
            .register_type::<crate::survey::SurveyAnomalyRegistry>()
            .register_type::<crate::survey::ModderAnomalyDef>()
            .register_type::<crate::survey::AnomalyEffect>()
            .register_type::<crate::survey::AnomalyDef>()
            .register_type::<crate::survey::MiningEfficiencyRegistry>()
            .register_type::<crate::survey::MiningEfficiencyRow>()
            .register_type::<crate::survey::AnalysisQueueIndex>()
            .register_type::<crate::survey::AnalysisJobRef>()
            .register_type::<crate::survey::RecoveryMissionKind>()
            .register_type::<crate::survey::RecoveryMission>()
            .register_type::<crate::survey::RecoveryMissionRegistry>()
            .register_type::<crate::survey::SurveyDimension>()
            .register_type::<crate::survey::SurveyMethod>()
            .register_type::<crate::survey::AnomalyType>()
            .register_type::<crate::survey::AnomalyState>()
            .register_type::<crate::survey::EvidencePoint>()
            .register_type::<crate::survey::EvidenceKind>()
            .register_type::<crate::survey::MissionStatus>()
            .register_type::<crate::survey::MissionFailureReason>()
            .register_type::<crate::survey::FailureKind>()
            // ── Shipbuilding ──────────────────────────────────────────
            .register_type::<crate::shipbuilding::ShipConstructionState>()
            .register_type::<crate::shipbuilding::ShipConstructionProject>()
            .register_type::<crate::shipbuilding::OrbitalStation>()
            .register_type::<crate::shipbuilding::ShipDesignAssignment>()
            .register_type::<crate::shipbuilding::PendingShipbuildingActions>()
            .register_type::<crate::shipbuilding::LaunchCapacityState>()
            .register_type::<crate::shipbuilding::Slipway>()
            .register_type::<crate::shipbuilding::ShipyardFacility>()
            .register_type::<crate::shipbuilding::RefitProject>()
            .register_type::<crate::shipbuilding::HullSlotDefinition>()
            .register_type::<crate::shipbuilding::ShipHullDefinition>()
            .register_type::<crate::shipbuilding::ShipModuleDefinition>()
            .register_type::<crate::shipbuilding::ShipbuildingData>()
            .register_type::<crate::shipbuilding::ShipDesignLibrary>()
            .register_type::<crate::shipbuilding::ShipDesignTemplate>()
            .register_type::<crate::shipbuilding::ShipModuleCategory>()
            .register_type::<crate::shipbuilding::HullSizeTier>()
            .register_type::<crate::shipbuilding::ConstructionMode>()
            // ── Personnel ──────────────────────────────────────────────
            .register_type::<crate::personnel::Scientist>()
            .register_type::<crate::personnel::ScientistSpecialty>()
            .register_type::<crate::personnel::SeniorityTier>()
            // ── Plugins (camera / atmosphere / ocean / starmap) ───────
            .register_type::<crate::plugins::camera::ViewMode>()
            .register_type::<crate::plugins::camera::EguiPanelBounds>()
            .register_type::<crate::plugins::camera::SavedSurveyRadius>()
            .register_type::<crate::plugins::camera::GameCamera>()
            .register_type::<crate::plugins::camera::CameraAnchor>()
            .register_type::<crate::plugins::camera::OrbitCamera>()
            .register_type::<crate::plugins::atmosphere::AtmosphereSettings>()
            .register_type::<crate::plugins::atmosphere::HasAtmosphereShell>()
            .register_type::<crate::plugins::atmosphere::AtmosphereShell>()
            .register_type::<crate::plugins::ocean::HasOceanShell>()
            .register_type::<crate::plugins::ocean::OceanShell>()
            .register_type::<crate::plugins::starmap::SystemMetadata>()
            .register_type::<crate::plugins::starmap::PlanetCategory>()
            .register_type::<crate::plugins::starmap::PlanetTextureManifest>()
            .register_type::<crate::plugins::starmap::StarSystemIcon>()
            .register_type::<crate::plugins::starmap::SolSystemIcon>()
            .register_type::<crate::plugins::starmap::HoveredStarSystem>()
            .register_type::<crate::plugins::starmap::SelectedStarSystem>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_is_one_in_pr_a() {
        assert_eq!(FORMAT_VERSION, 1);
        assert_eq!(MIN_SUPPORTED_VERSION, 1);
    }

    #[test]
    fn plugin_registers_type_registry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(PersistencePlugin);
        assert!(app.world().contains_resource::<AppTypeRegistry>());
    }
}
