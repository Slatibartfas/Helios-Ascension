//! Helios Ascension app builder.
//!
//! Single source of truth for the plugin order. Both `main.rs` (the live
//! game) and `bin/screenshot.rs` (the headless capture) call
//! [`build_helios_app`] so they cannot drift apart.
//!
//! Custom-window behaviour (live play vs. headless capture) is layered in
//! by the caller, not by this function.

use bevy::prelude::*;
#[cfg(target_os = "windows")]
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::{PresentMode, WindowResizeConstraints, WindowResolution};
use bevy_egui::EguiPlugin;

use crate::astronomy::AstronomyPlugin;
use crate::colony::ColonyPlugin;
use crate::economy::EconomyPlugin;
use crate::fleets::FleetPlugin;
use crate::game_state::GameStatePlugin;
use crate::plugins::{
    atmosphere::AtmospherePlugin, camera::CameraPlugin, music::MusicPlugin,
    solar_system::SolarSystemPlugin, starmap::StarmapPlugin,
    system_populator::SystemPopulatorPlugin, visual_effects::VisualEffectsPlugin,
};
use crate::render::backdrop::BackdropPlugin;
use crate::research::ResearchPlugin;
use crate::shipbuilding::ShipbuildingPlugin;
use crate::ui::UIPlugin;

/// Minimum supported window dimensions. Below 720p the layout clamps and
/// panels overlap, so we refuse to shrink.
pub const MIN_WINDOW_WIDTH: f32 = 1280.0;
pub const MIN_WINDOW_HEIGHT: f32 = 720.0;

/// Construct a fully-configured Helios Ascension `App`. The caller owns the
/// returned app and decides whether to `.run()` it, drive a fixed update
/// loop, or hand it to a test harness.
pub fn build_helios_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Helios Ascension".to_string(),
                    resolution: WindowResolution::new(1920, 1080),
                    present_mode: PresentMode::Fifo,
                    resize_constraints: WindowResizeConstraints {
                        min_width: MIN_WINDOW_WIDTH,
                        min_height: MIN_WINDOW_HEIGHT,
                        ..default()
                    },
                    ..default()
                }),
                ..default()
            })
            .set(render_plugin_settings()),
    )
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
    // Default scene setup (ambient light, background fill)
    .add_systems(Startup, setup_default_scene);
    app
}

/// Default scene setup shared by every entry point. In the live game this
/// creates the deep-space background; the screenshot binary relies on it
/// so the captured frames are not pitch-black.
pub fn setup_default_scene(mut commands: Commands, mut clear_color: Option<ResMut<ClearColor>>) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.7, 0.75, 1.0),
        brightness: 8.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.6, 0.65, 0.8),
            illuminance: 100.0,
            ..default()
        },
        Transform::from_xyz(1.0, 0.5, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    if let Some(mut cc) = clear_color {
        cc.0 = Color::srgb(0.01, 0.01, 0.02);
    } else {
        commands.insert_resource(ClearColor(Color::srgb(0.01, 0.01, 0.02)));
    }
}

fn render_plugin_settings() -> RenderPlugin {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut plugin = RenderPlugin::default();

    #[cfg(target_os = "windows")]
    {
        // Intel Arc + Vulkan has been intermittently failing during swap-chain acquisition
        // on startup. Prefer DX12 on Windows to avoid the unrecoverable surface loss path.
        plugin.render_creation = RenderCreation::Automatic(WgpuSettings {
            backends: Some(Backends::DX12),
            ..default()
        });
    }

    plugin
}
