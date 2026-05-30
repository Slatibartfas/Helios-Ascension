//! Diplomacy system — treaties, faction relations, and diplomatic AI.
//!
//! # Architecture
//!
//! - `components`: FactionRelation, Treaty, DiplomaticProposal, DiplomaticVictoryTracker
//! - `systems`: Reputation updates, treaty compliance, violation detection, proposal evaluation
//! - `plugin`: Bevy plugin registration
//!
//! # Key Concepts
//!
//! - **FactionRelation**: per-ordered-pair relationship state (reputation, stance, treaties)
//! - **Treaty**: active agreement with effects and violation tracking
//! - **DiplomaticProposal**: pending proposal with offer/demand terms
//! - **RelationsGraph**: resource holding all FactionRelation components

pub mod components;
pub mod systems;
pub mod plugin;

pub use components::*;
pub use systems::*;
pub use plugin::*;