//! Victory condition types and data structures.

use bevy::prelude::*;

/// Type of victory achieved (or in progress toward).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VictoryType {
    #[default]
    None,
    Scientific,
    Military,
    Economic,
    Diplomatic,
}

impl VictoryType {
    /// Display name for this victory type.
    pub fn display_name(&self) -> &'static str {
        match self {
            VictoryType::None => "",
            VictoryType::Scientific => "Scientific Victory",
            VictoryType::Military => "Military Victory",
            VictoryType::Economic => "Economic Victory",
            VictoryType::Diplomatic => "Diplomatic Victory",
        }
    }

    /// Theme colour hex for this victory type (for UI).
    pub fn colour(&self) -> [f32; 4] {
        match self {
            VictoryType::None => [0.5, 0.5, 0.5, 1.0],
            VictoryType::Scientific => [0.29, 0.56, 0.85, 1.0],  // #4A90D9
            VictoryType::Military => [0.85, 0.29, 0.29, 1.0],     // #D94A4A
            VictoryType::Economic => [0.85, 0.72, 0.29, 1.0],    // #D9B84A
            VictoryType::Diplomatic => [0.29, 0.85, 0.54, 1.0], // #4AD98A
        }
    }

    /// Icon for this victory type.
    pub fn icon(&self) -> &'static str {
        match self {
            VictoryType::None => "",
            VictoryType::Scientific => "🔬",
            VictoryType::Military => "⚔️",
            VictoryType::Economic => "💰",
            VictoryType::Diplomatic => "🤝",
        }
    }
}

/// Tracks the current progress toward each victory condition and whether
/// a victory has been claimed.
#[derive(Resource, Debug, Clone, Default)]
pub struct VictoryState {
    /// Set to true the frame a scientific victory is achieved.
    pub scientific_victory_achieved: bool,
    /// Set to true the frame a military victory is achieved.
    pub military_victory_achieved: bool,
    /// Set to true the frame an economic victory is achieved.
    pub economic_victory_achieved: bool,
    /// Set to true the frame a diplomatic victory is achieved.
    pub diplomatic_victory_achieved: bool,
    /// Which faction first achieved a victory condition (None = no victory yet).
    pub first_victor: Option<FactionId>,
    /// Simulation time (seconds) when the first victory was claimed.
    pub first_victory_time: Option<f64>,
    /// Set to true to show the full-screen endgame overlay.
    pub endgame_screen_visible: bool,
    /// Which victory type was achieved (for endgame screen display).
    pub achieved_victory_type: VictoryType,
    /// Whether the current (human player) faction has achieved this victory.
    pub player_has_won: bool,
}

/// Simplified faction identifier used in victory tracking.
/// 0 = human player; 1+ = AI faction IDs.
pub type FactionId = u32;

impl VictoryState {
    /// Returns true if any victory condition has been achieved.
    pub fn any_victory_achieved(&self) -> bool {
        self.scientific_victory_achieved
            || self.military_victory_achieved
            || self.economic_victory_achieved
            || self.diplomatic_victory_achieved
    }

    /// Mark a victory as achieved for the given faction.
    pub fn claim_victory(&mut self, faction: FactionId, victory_type: VictoryType, time: f64) {
        if self.first_victor.is_some() {
            // A victory was already claimed — another faction finished first.
            // Partial victory: the first to finish wins, but the game continues.
            return;
        }

        self.first_victor = Some(faction);
        self.first_victory_time = Some(time);
        self.achieved_victory_type = victory_type;

        match victory_type {
            VictoryType::Scientific => self.scientific_victory_achieved = true,
            VictoryType::Military => self.military_victory_achieved = true,
            VictoryType::Economic => self.economic_victory_achieved = true,
            VictoryType::Diplomatic => self.diplomatic_victory_achieved = true,
            VictoryType::None => {}
        }

        self.endgame_screen_visible = true;
        self.player_has_won = (faction == 0);
    }

    /// Check if the player is the first to achieve any victory.
    pub fn player_claimed_first_victory(&self) -> bool {
        self.first_victor == Some(0)
    }
}