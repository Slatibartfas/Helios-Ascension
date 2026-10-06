//! Mission log — global, persistent record of current / past missions and
//! long-running goals.
//!
//! See [GRA-791](https://paperclip.klingspor.one/GRA/issues/GRA-791) for the
//! design contract. This module is the data layer; UI rendering is owned by
//! the UX Designer follow-up.
//!
//! Module map:
//!
//! - [`components`] — `MissionLog` resource, `MissionLogConfig` resource,
//!   `MissionEntry`, `GoalEntry`, enums.
//! - [`systems`] — consumer systems that translate the sim-layer event
//!   surface (`SurveyEvent`, `ConstructionEvent`, `ResearchEvent`, plus
//!   the future `MilestoneReached` from GRA-804) into mutations on
//!   `MissionLog`. All run in `Update`.
//! - [`plugin`] — `MissionLogPlugin` wires the resources, system sets,
//!   and consumer systems.
//!
//! # Read-only contract
//!
//! `MissionLog` is owned by the simulation writers declared in
//! [`systems`]. UI systems MUST read it via `Res<MissionLog>` only.
//! Writing to `MissionLog` from `src/ui/**` is a layering violation;
//! the [`systems::assert_no_ui_resmut`] helper documents and
//! regression-tests this contract.

#![allow(clippy::module_inception)]

pub mod components;
pub mod plugin;
pub mod systems;

pub use components::{
    GoalEntry, GoalStatus, MissionEntry, MissionKind, MissionLog, MissionLogConfig, MissionOutcome,
    MissionSource,
};
pub use plugin::MissionLogPlugin;
pub use systems::MissionLogSystemSet;
