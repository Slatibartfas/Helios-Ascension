use bevy::log::LogPlugin;
use bevy::prelude::*;
#[cfg(target_os = "windows")]
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::{ExitCondition, PresentMode, WindowResizeConstraints, WindowResolution};
use bevy_egui::EguiPlugin;

pub mod astronomy;
pub mod boot_init;
pub mod colony;
pub mod economy;
pub mod fleets;
pub mod game_state;
pub mod persistence;
pub mod personnel;
pub mod plugins;
pub mod render;
pub mod research;
pub mod shipbuilding;
pub mod ships;
pub mod survey;
pub mod ui;

use astronomy::AstronomyPlugin;
use boot_init::BootInitPlugin;
use colony::ColonyPlugin;
use economy::EconomyPlugin;
use fleets::FleetPlugin;
use game_state::GameStatePlugin;
use persistence::{GameSetupPlugin, PersistencePlugin, SaveLoadPlugin};
use personnel::PersonnelPlugin;
use plugins::{
    atmosphere::AtmospherePlugin, camera::CameraPlugin, music::MusicPlugin,
    solar_system::SolarSystemPlugin, starmap::StarmapPlugin,
    system_populator::SystemPopulatorPlugin, visual_effects::VisualEffectsPlugin,
    window_icon::WindowIconPlugin,
};
use render::backdrop::BackdropPlugin;
use research::ResearchPlugin;
use shipbuilding::ShipbuildingPlugin;
use survey::SurveyPlugin;
use ui::launch::SplashPlugin;
use ui::UIPlugin;

/// Minimum supported window dimensions for the main game.
///
/// The UI is now responsive enough to remain usable below 1080p, which
/// avoids forcing oversized swap chains on smaller Windows displays.
const MIN_WINDOW_WIDTH: f32 = 1280.0;
const MIN_WINDOW_HEIGHT: f32 = 720.0;

/// Entry point: build the game app and run it. The splash lives
/// inside the same Bevy app as the main game (see
/// [`crate::ui::launch::splash`]) — winit 0.30 forbids creating a
/// second `EventLoop` after the first exits, so a "pre-main splash
/// Bevy app" isn't an option. The splash uses a separate `Window`
/// entity, sized to the logo PNG and visible by default; the main
/// game window exists from boot but has `visible: false`. Splash
/// dismissal flips the two visibility bits.
///
/// See [`build_game_app`] for the main game configuration and
/// [`SplashPlugin`] for the splash window + camera setup.
fn main() {
    let mut app = build_game_app();
    app.run();
}

/// Build the main game Bevy app. Pulled out of `main` so the splash
/// boot (above) and the game boot share a clean top-level structure
/// without inlining the entire app in `main`.
fn build_game_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        // Bevy default plugins with custom window configuration.
        // The main window is `visible: false` at boot — the splash
        // window owns the screen until it dismisses
        // ([`crate::ui::launch::splash::ui_splash_system`] flips the
        // bit). Keeping the window hidden instead of not spawning it
        // avoids the winit event-loop-recreation issue described in
        // `main`.
        DefaultPlugins
            // Keep normal Bevy/application startup information while
            // suppressing DX12's generated HLSL source and Naga's
            // per-binding translation diagnostics. Both are emitted
            // at `info` during normal shader compilation and can
            // otherwise produce thousands of startup lines.
            .set(LogPlugin {
                filter: "info,helios_ascension=info,wgpu_hal::dx12::device=warn,naga::back::hlsl::writer=warn".to_string(),
                level: bevy::log::Level::INFO,
                custom_layer: |_app| None,
                fmt_layer: |_app| None,
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Helios Ascension".to_string(),
                    resolution: WindowResolution::new(1920, 1080),
                    present_mode: PresentMode::Fifo,
                    visible: false, // hidden until splash dismisses
                    resize_constraints: WindowResizeConstraints {
                        min_width: MIN_WINDOW_WIDTH,
                        min_height: MIN_WINDOW_HEIGHT,
                        ..default()
                    },
                    ..default()
                }),
                // The dismissed splash window remains hidden until shutdown so
                // Windows can deliver its final native events while Bevy still
                // owns the WindowId mapping. Closing the primary window should
                // therefore end the app even though that hidden window exists.
                exit_condition: ExitCondition::OnPrimaryClosed,
                ..default()
            })
            .set(render_plugin_settings()),
    )
    // Debug UI (egui)
    .add_plugins(EguiPlugin::default())
    // Splash plugin — registered BEFORE LaunchPlugin so the splash
    // window is spawned before the main menu's render system runs.
    // The main menu's render is gated on `LaunchState::MainMenu`,
    // so it no-ops during the splash — but the splash window itself
    // must be set up first to be visible.
    .add_plugins(SplashPlugin)
    // Boot-init plugin — defers game-state init (solar system,
    // 60-system population, baseline tech / engineering / debug
    // fleet, camera focus, asteroid registry, resource generation)
    // into `Update`, gated by `BootState::Loading`. The splash
    // window dismisses once `BootState` flips to `Ready`, so the
    // boot-init work happens behind the splash. See
    // `src/boot_init.rs` for the chain order.
    .add_plugins(BootInitPlugin)
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
    .add_plugins(SurveyPlugin)
    .add_plugins(PersonnelPlugin)
    .add_plugins(FleetPlugin)
    .add_plugins(ShipbuildingPlugin)
    .add_plugins(SystemPopulatorPlugin)
    .add_plugins(UIPlugin)
    .add_plugins(PersistencePlugin)
    .add_plugins(SaveLoadPlugin)
    .add_plugins(GameSetupPlugin)
    .add_plugins(crate::persistence::RestoreDecorationPlugin)
    // Window/taskbar icon — must register *after* WindowPlugin and
    // SplashPlugin so the primary + splash window entities exist in
    // the world when the Startup system runs.
    .add_plugins(WindowIconPlugin)
    .add_plugins(MusicPlugin)
    // Systems
    .add_systems(Startup, setup);
    app
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
