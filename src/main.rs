use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

pub mod astronomy;
pub mod economy;
pub mod plugins;
pub mod ui;

use astronomy::AstronomyPlugin;
use economy::EconomyPlugin;
use plugins::{
    camera::CameraPlugin, 
    solar_system::SolarSystemPlugin,
    visual_effects::VisualEffectsPlugin,
};
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
        // Debug UI
        .add_plugins(WorldInspectorPlugin::new())
        // Game plugins - Order matters for dependencies
        .add_plugins(AstronomyPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(VisualEffectsPlugin)
        .add_plugins(SolarSystemPlugin)
        .add_plugins(EconomyPlugin)
        .add_plugins(UIPlugin)
        // Systems
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Add ambient light for space atmosphere - increased for better visibility
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.7, 0.75, 0.85), // Slightly blue-tinted for space
        brightness: 0.3, // Increased from 0.1 for better body visibility
    });
    
    // Set clear color to deep black for space
    commands.insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.02)));
}
