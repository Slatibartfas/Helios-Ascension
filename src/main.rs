use bevy::prelude::*;
use bevy::window::{WindowResizeConstraints, WindowResolution};
use bevy_egui::EguiPlugin;

pub mod astronomy;
pub mod colony;
pub mod economy;
pub mod fleets;
pub mod game_state;
pub mod plugins;
pub mod render;
pub mod research;
pub mod shipbuilding;
pub mod ui;

use astronomy::AstronomyPlugin;
use colony::ColonyPlugin;
use economy::EconomyPlugin;
use fleets::FleetPlugin;
use game_state::GameStatePlugin;
use plugins::{
    atmosphere::AtmospherePlugin, camera::CameraPlugin, music::MusicPlugin,
    solar_system::SolarSystemPlugin, starmap::StarmapPlugin,
    system_populator::SystemPopulatorPlugin, visual_effects::VisualEffectsPlugin,
};
use render::backdrop::BackdropPlugin;
use research::ResearchPlugin;
use shipbuilding::ShipbuildingPlugin;
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
        .add_plugins(AtmospherePlugin)
        // OceanPlugin disabled — the shader doesn't account for sun direction,
        // causing uniform brightening on both day and night sides.  Ocean data
        // (OceanProperties component) is still inserted for UI display.
        .add_plugins(SolarSystemPlugin)
        .add_plugins(StarmapPlugin)
        .add_plugins(EconomyPlugin)
        .add_plugins(ColonyPlugin)
        .add_plugins(ResearchPlugin)
        .add_plugins(FleetPlugin)
        .add_plugins(ShipbuildingPlugin)
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
    // 8 lux provides enough fill that night sides of distant/dim-star
    // planets are faintly visible rather than pitch black, while still
    // being dominated by the star's PointLight on the day side.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.7, 0.75, 1.0), // Cool blue-white starlight tint
        brightness: 8.0,
        ..default()
    });

    // Add a subtle directional "galactic background" light that provides
    // uniform fill illumination regardless of distance from the star.
    // This simulates scattered interstellar starlight so distant bodies
    // (dwarf planets, Kuiper belt objects) aren't pitch black.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.6, 0.65, 0.8), // Slightly cool galactic tint
            // Low intensity - just enough to give distant bodies visibility.
            // Combined with GlobalAmbientLight (8 lux) this should keep
            // inner planets well-lit while making outer planets visible.
            illuminance: 100.0,
            ..default()
        },
        // Point away from the system plane for natural-looking fill
        Transform::from_xyz(1.0, 0.5, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Set clear color to deep black for space
    commands.insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.02)));
}
