use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct VisualEffectsPlugin;

impl Plugin for VisualEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera_effects);
        app.add_plugins(MaterialPlugin::<NightMaterial>::default());
    }
}

/// Material for night-side textures (city lights)
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct NightMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub night_texture: Handle<Image>,
    #[uniform(2)]
    pub sun_position: Vec4,
}

impl Material for NightMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/night_material.wgsl".into()
    }

    // Set transparency mode to additive blending
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
}

/// Setup camera effects for better space atmosphere
fn setup_camera_effects(mut commands: Commands, camera_query: Query<Entity, With<Camera3d>>) {
    if let Ok(camera_entity) = camera_query.single() {
        // Add bloom effect for bright objects (stars, sun) — tuned for subtle, realistic corona
        commands.entity(camera_entity).insert((
            Bloom {
                intensity: 0.25,
                // Reduced from 0.6: a narrower low-frequency boost means the bloom halo
                // stays close to the star surface and doesn't spread across close-in orbits.
                low_frequency_boost: 0.35,
                low_frequency_boost_curvature: 0.4,
                high_pass_frequency: 0.1,
                prefilter: bevy::post_process::bloom::BloomPrefilter {
                    // Raised from 2.0: only genuine star-surface HDR values (≥3.0) trigger
                    // bloom — dim planet surfaces illuminated by a nearby star no longer
                    // exceed the threshold and develop their own halo.
                    threshold: 3.0,
                    threshold_softness: 0.4,
                },
                composite_mode: bevy::post_process::bloom::BloomCompositeMode::Additive,
                ..default()
            },
            Tonemapping::ReinhardLuminance, // Better for handling extreme dynamic range
        ));
    }
}
