//! Game settings that persist to save files.
//!
//! All settings use serde serialization so they can be persisted through
//! the game's save/load system.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Global game settings that persist across sessions.
#[derive(Resource, Serialize, Deserialize, Clone, Debug)]
pub struct GameSettings {
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub difficulty: DifficultySettings,
    pub gameplay: GameplaySettings,
    pub keybindings: KeybindingsSettings,
    pub ui: UiSettings,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            graphics: GraphicsSettings::default(),
            audio: AudioSettings::default(),
            difficulty: DifficultySettings::default(),
            gameplay: GameplaySettings::default(),
            keybindings: KeybindingsSettings::default(),
            ui: UiSettings::default(),
        }
    }
}

/// Graphics display settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GraphicsSettings {
    /// Display resolution width in pixels.
    pub resolution_width: u32,
    /// Display resolution height in pixels.
    pub resolution_height: u32,
    /// Whether to use fullscreen mode.
    pub fullscreen: bool,
    /// Whether vertical sync is enabled.
    pub vsync: bool,
    /// Render distance as a multiplier (0.5 = half, 2.0 = double).
    pub render_distance: f32,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            resolution_width: 1920,
            resolution_height: 1080,
            fullscreen: false,
            vsync: true,
            render_distance: 1.0,
        }
    }
}

/// Audio volume and mute settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AudioSettings {
    /// Music volume (0.0 to 1.0).
    pub music_volume: f32,
    /// Sound effects volume (0.0 to 1.0).
    pub sfx_volume: f32,
    /// Whether all audio is muted.
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.7,
            sfx_volume: 0.8,
            muted: false,
        }
    }
}

/// Difficulty level settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DifficultySettings {
    /// The current difficulty level.
    pub difficulty: DifficultyLevel,
    /// Custom difficulty modifiers (used when difficulty is Custom).
    pub custom_modifiers: CustomDifficultyModifiers,
}

impl Default for DifficultySettings {
    fn default() -> Self {
        Self {
            difficulty: DifficultyLevel::Normal,
            custom_modifiers: CustomDifficultyModifiers::default(),
        }
    }
}

/// Predefined difficulty levels.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DifficultyLevel {
    #[default]
    Easy,
    Normal,
    Hard,
    Custom,
}

/// Custom difficulty modifiers when difficulty is set to Custom.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomDifficultyModifiers {
    /// AI production efficiency multiplier (0.0 to 2.0).
    pub ai_efficiency: f32,
    /// Resource generation rate multiplier (0.0 to 2.0).
    pub resource_rate: f32,
    /// Research speed multiplier (0.0 to 2.0).
    pub research_speed: f32,
    /// Enemy aggression multiplier (0.0 to 2.0).
    pub enemy_aggression: f32,
}

impl Default for CustomDifficultyModifiers {
    fn default() -> Self {
        Self {
            ai_efficiency: 1.0,
            resource_rate: 1.0,
            research_speed: 1.0,
            enemy_aggression: 1.0,
        }
    }
}

/// Gameplay and simulation settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GameplaySettings {
    /// Maximum simulation speed multiplier (e.g., 1.0 = 1x, 10.0 = 10x).
    pub simulation_speed_cap: f32,
    /// Auto-save interval in seconds.
    pub auto_save_interval_seconds: u64,
    /// Whether the tutorial system is enabled.
    pub tutorial_enabled: bool,
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            simulation_speed_cap: 10.0,
            auto_save_interval_seconds: 300, // 5 minutes
            tutorial_enabled: true,
        }
    }
}

/// Keybinding settings for remappable controls.
/// Stored as string names for serde compatibility (bevy::prelude::KeyCode does not
/// implement Serialize/Deserialize).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeybindingsSettings {
    pub camera_up: Option<String>,
    pub camera_down: Option<String>,
    pub camera_left: Option<String>,
    pub camera_right: Option<String>,
    pub camera_zoom_in: Option<String>,
    pub camera_zoom_out: Option<String>,
    pub camera_recenter: Option<String>,
    pub fleet_menu: Option<String>,
    pub transfer_planner: Option<String>,
    pub pause: Option<String>,
}

impl Default for KeybindingsSettings {
    fn default() -> Self {
        Self {
            camera_up: Some("KeyW".to_string()),
            camera_down: Some("KeyS".to_string()),
            camera_left: Some("KeyA".to_string()),
            camera_right: Some("KeyD".to_string()),
            camera_zoom_in: Some("KeyE".to_string()),
            camera_zoom_out: Some("KeyQ".to_string()),
            camera_recenter: Some("Home".to_string()),
            fleet_menu: Some("KeyF".to_string()),
            transfer_planner: Some("KeyT".to_string()),
            pause: Some("Space".to_string()),
        }
    }
}

/// UI appearance settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UiSettings {
    /// UI theme selection.
    pub theme: UiTheme,
    /// Font size scaling factor (1.0 = default, 1.25 = 125% larger).
    pub font_scale: f32,
    /// Color blind mode for accessible palettes.
    pub color_blind_mode: ColorBlindMode,
    /// UI scale factor for accessibility (0.75 to 2.0, default 1.0).
    pub ui_scale: f32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: UiTheme::Dark,
            font_scale: 1.0,
            color_blind_mode: ColorBlindMode::None,
            ui_scale: 1.0,
        }
    }
}

/// Available UI themes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiTheme {
    #[default]
    Dark,
    Light,
}

/// Color blind mode for accessible palettes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorBlindMode {
    /// Standard color palette (default)
    #[default]
    None,
    /// Red-green color blindness (most common)
    Deuteranopia,
    /// Red-green color blindness (different variant)
    Protanopia,
    /// Blue-yellow color blindness
    Tritanopia,
}

impl ColorBlindMode {
    /// Get display name for the color blind mode.
    pub fn name(&self) -> &'static str {
        match self {
            ColorBlindMode::None => "Off",
            ColorBlindMode::Deuteranopia => "Deuteranopia (Red-Green)",
            ColorBlindMode::Protanopia => "Protanopia (Red-Green)",
            ColorBlindMode::Tritanopia => "Tritanopia (Blue-Yellow)",
        }
    }

    /// Get all modes as a slice for UI selection.
    pub fn all() -> &'static [ColorBlindMode] {
        &[
            ColorBlindMode::None,
            ColorBlindMode::Deuteranopia,
            ColorBlindMode::Protanopia,
            ColorBlindMode::Tritanopia,
        ]
    }
}