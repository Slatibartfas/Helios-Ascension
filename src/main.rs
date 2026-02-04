use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

pub mod astronomy;
pub mod plugins;

use astronomy::AstronomyPlugin;
use plugins::{
    camera::CameraPlugin, 
    solar_system::SolarSystemPlugin,
    visual_effects::VisualEffectsPlugin,
};

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
        // Game plugins - CameraPlugin must be before VisualEffectsPlugin
        .add_plugins(AstronomyPlugin)
        .add_plugins(CameraPlugin)
        .add_plugins(VisualEffectsPlugin)
        .add_plugins(SolarSystemPlugin)
        // Systems
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Add subtle ambient light for space atmosphere
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.7, 0.75, 0.85), // Slightly blue-tinted for space
        brightness: 0.1, // Much dimmer for space ambience
    });
    
    // Set clear color to deep black for space
    commands.insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.02)));
}
