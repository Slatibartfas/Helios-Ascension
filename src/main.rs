use bevy::prelude::*;
use bevy::window::{WindowResolution, WindowResizeConstraints};
use bevy_egui::EguiPlugin;

pub mod astronomy;
pub mod colony;
pub mod economy;
pub mod game_state;
pub mod plugins;
pub mod render;
pub mod research;
pub mod ui;

use astronomy::AstronomyPlugin;
use colony::ColonyPlugin;
use economy::EconomyPlugin;
use game_state::GameStatePlugin;
use research::ResearchPlugin;
use plugins::{
    camera::CameraPlugin, music::MusicPlugin, solar_system::SolarSystemPlugin,
    starmap::StarmapPlugin, system_populator::SystemPopulatorPlugin,
    visual_effects::VisualEffectsPlugin,
};
use render::backdrop::BackdropPlugin;
use ui::UIPlugin;

/// Minimum supported window dimensions to prevent UI overlap
/// Full HD (1920×1080) is required for the complex strategy game UI
const MIN_WINDOW_WIDTH: f32 = 1920.0;
const MIN_WINDOW_HEIGHT: f32 = 1080.0;

fn main() {
    App::new()
        // Bevy default plugins with custom window configuration
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Helios Ascension".to_string(),
                resolution: WindowResolution::new(1920, 1080),
                resize_constraints: WindowResizeConstraints {
                    min_width: MIN_WINDOW_WIDTH,
                    min_height: MIN_WINDOW_HEIGHT,
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        // Debug UI (egui)
        .add_plugins(EguiPlugin::default())
        // Game plugins - Order matters for dependencies
        .add_plugins(GameStatePlugin)
        .add_plugins(AstronomyPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(BackdropPlugin)
        .add_plugins(VisualEffectsPlugin)
        .add_plugins(SolarSystemPlugin)
        .add_plugins(StarmapPlugin)
        .add_plugins(EconomyPlugin)
        .add_plugins(ColonyPlugin)
        .add_plugins(ResearchPlugin)
        .add_plugins(SystemPopulatorPlugin)
        .add_plugins(UIPlugin)
        .add_plugins(MusicPlugin)
        // Systems
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Add ambient light for space atmosphere
    // In Bevy 0.14, brightness is measured in lux (default: 80.0).
    // 30 lux provides enough fill light so textures are visible on all bodies,
    // while still allowing the Sun's point-light to create clear day/night contrast.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.9, 0.92, 1.0), // Neutral to slightly blue for space
        brightness: 30.0,
        ..default()
    });

    // Set clear color to deep black for space
    commands.insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.02)));
}
