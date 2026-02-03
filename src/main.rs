use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

pub mod plugins;

use plugins::{camera::CameraPlugin, solar_system::SolarSystemPlugin};

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
        // Game plugins
        .add_plugins(CameraPlugin)
        .add_plugins(SolarSystemPlugin)
        // Systems
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Add ambient light for better visibility
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.3,
    });
}
