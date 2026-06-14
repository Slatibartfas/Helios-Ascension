//! Research Bevy `Message` types — emitted by the research sim system
//! and consumed by the notifications surface (PR-C, GRA-137) and the
//! research panel.
//!
//! PR-C (GRA-137) adds `ResearchEvent` to wire the research layer into
//! the notifications plugin's `EventBridge` system set. The single
//! variant today mirrors the issue spec:
//!
//! - `TechCompleted { tech_id, tech_display_name }` — a research project
//!   hit 100% progress. Fired from `advance_research_projects` at the
//!   `completed_projects` drain loop.
//!
//! `engineering_facility` completion is intentionally not modelled here
//! yet — the engineering flow has its own completion system and
//! `EngineeringProject`, but the GRA-134 design comment scopes PR-C to
//! research only. A future ticket can add `EngineeringProjectCompleted`
//! when the engineering notification surface is specified.

use bevy::prelude::*;

/// Research state transitions emitted to the message bus.
#[derive(Message, Debug, Clone)]
pub enum ResearchEvent {
    /// A research project hit 100% and the tech was unlocked.
    TechCompleted {
        /// Stable id from `assets/data/technologies.ron` (e.g.
        /// `fusion_propulsion`).
        tech_id: String,
        /// Player-facing display name (e.g. "Fusion Propulsion"). The
        /// bridge uses this in the toast title so the player doesn't
        /// have to look up the id.
        tech_display_name: String,
    },
}
