use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_egui::EguiPlugin;

pub mod astronomy;
pub mod economy;
pub mod plugins;
pub mod render;
pub mod ui;

use astronomy::AstronomyPlugin;
use economy::EconomyPlugin;
use plugins::{
    camera::CameraPlugin,
    solar_system::SolarSystemPlugin,
    starmap::StarmapPlugin,
    system_populator::SystemPopulatorPlugin,
    visual_effects::VisualEffectsPlugin,
};
use render::backdrop::BackdropPlugin;
use ui::UIPlugin;

fn main() {
    App::new()
        // Bevy default plugins with custom window configuration
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Helios Ascension".to_string(),
                resolution: WindowResolution::new(1920.0, 1080.0),
                ..default()
            }),
            ..default()
        }))
        // Debug UI (egui)
        .add_plugins(EguiPlugin)
        // Game plugins - Order matters for dependencies
        .add_plugins(AstronomyPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(BackdropPlugin)
        .add_plugins(VisualEffectsPlugin)
        .add_plugins(SolarSystemPlugin)
        .add_plugins(StarmapPlugin)
        .add_plugins(EconomyPlugin)
        .add_plugins(SystemPopulatorPlugin)
        .add_plugins(UIPlugin)
        // Systems
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Add ambient light for space atmosphere
    // In Bevy 0.14, brightness is measured in lux (default: 80.0).
    // 30 lux provides enough fill light so textures are visible on all bodies,
    // while still allowing the Sun's point-light to create clear day/night contrast.
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.9, 0.92, 1.0), // Neutral to slightly blue for space
        brightness: 30.0,
    });
    
    // Set clear color to deep black for space
    commands.insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.02)));
}
