use bevy::prelude::*;
use bevy::core_pipeline::bloom::BloomSettings;
use bevy::core_pipeline::tonemapping::Tonemapping;
use rand::Rng;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};

pub struct VisualEffectsPlugin;

impl Plugin for VisualEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_starfield, setup_camera_effects));
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

/// Component for starfield background particles
#[derive(Component)]
pub struct StarParticle {
    pub brightness: f32,
    pub size: f32,
}

/// Setup a procedural starfield background
fn setup_starfield(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = rand::thread_rng();
    
    // Create multiple layers of stars at different distances
    let layers = vec![
        (2000, 5000.0, 0.3, 0.8),   // Distant dim stars
        (1000, 3000.0, 0.5, 1.2),   // Medium distance stars
        (500, 1500.0, 0.8, 1.5),    // Closer brighter stars
        (200, 800.0, 1.0, 2.0),     // Close large stars
    ];
    
    for (count, distance, base_brightness, base_size) in layers {
        for _ in 0..count {
            // Random position on a sphere
            let theta = rng.gen::<f32>() * std::f32::consts::TAU;
            let phi = rng.gen::<f32>() * std::f32::consts::PI;
            
            let x = distance * phi.sin() * theta.cos();
            let y = distance * phi.sin() * theta.sin();
            let z = distance * phi.cos();
            
            // Vary brightness and size
            let brightness_variance = rng.gen::<f32>() * 0.5 + 0.5;
            let size_variance = rng.gen::<f32>() * 0.5 + 0.5;
            
            let brightness = base_brightness * brightness_variance;
            let size = base_size * size_variance;
            
            // Star color distribution: 75% white, 10% blue, 10% yellow/orange, 5% red
            let color_temp = rng.gen::<f32>();
            let star_color = if color_temp < 0.10 {
                // Blue stars (hot) - 10%
                Color::srgb(0.7, 0.8, 1.0)
            } else if color_temp < 0.20 {
                // Yellow/orange stars - 10%
                Color::srgb(1.0, 0.9, 0.7)
            } else if color_temp < 0.25 {
                // Red stars (cool) - 5%
                Color::srgb(1.0, 0.7, 0.6)
            } else {
                // White stars (most common) - 75%
                Color::srgb(1.0, 1.0, 1.0)
            };
            
            // Create star mesh
            let star_mesh = meshes.add(Sphere::new(size));
            let star_material = materials.add(StandardMaterial {
                base_color: star_color,
                emissive: LinearRgba::from(star_color) * brightness,
                unlit: true,
                ..default()
            });
            
            commands.spawn((
                PbrBundle {
                    mesh: star_mesh,
                    material: star_material,
                    transform: Transform::from_xyz(x, y, z),
                    ..default()
                },
                StarParticle {
                    brightness,
                    size,
                },
            ));
        }
    }
    
    // Add a subtle nebula effect using a large semi-transparent sphere with gradient
    // create_nebula_backdrop(&mut commands, &mut meshes, &mut materials); // Disabled per user feedback (looks like weird circles)
}

/// Create a subtle nebula background
#[allow(dead_code)]
fn create_nebula_backdrop(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = rand::thread_rng();
    
    // Create large nebula clouds for a rich space backdrop
    let nebula_positions = vec![
        // Large distant nebulae
        (Vec3::new(4000.0, 800.0, 3000.0), Color::srgba(0.25, 0.10, 0.35, 0.04), 1800.0),    // Deep purple
        (Vec3::new(-3500.0, -1200.0, 2500.0), Color::srgba(0.10, 0.18, 0.40, 0.035), 2000.0), // Deep blue
        (Vec3::new(1500.0, -2500.0, -3500.0), Color::srgba(0.40, 0.12, 0.22, 0.03), 1600.0),  // Crimson
        (Vec3::new(-2000.0, 3000.0, -2500.0), Color::srgba(0.35, 0.25, 0.10, 0.025), 1900.0), // Amber
        // Medium clouds for mid-field depth
        (Vec3::new(2200.0, -600.0, 1800.0), Color::srgba(0.15, 0.20, 0.45, 0.03), 1200.0),   // Violet-blue
        (Vec3::new(-1800.0, 1500.0, 2200.0), Color::srgba(0.30, 0.08, 0.30, 0.035), 1400.0),  // Magenta
        (Vec3::new(500.0, 2800.0, -1500.0), Color::srgba(0.08, 0.25, 0.35, 0.025), 1500.0),   // Teal
        (Vec3::new(-3000.0, -800.0, -1800.0), Color::srgba(0.20, 0.15, 0.40, 0.03), 1700.0),  // Lavender
    ];
    
    for (position, color, size) in nebula_positions {
        let size_varied = size + rng.gen::<f32>() * 400.0;
        let nebula_mesh = meshes.add(Sphere::new(size_varied));
        let nebula_material = materials.add(StandardMaterial {
            base_color: color,
            emissive: LinearRgba::from(color) * 0.8,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        
        commands.spawn(PbrBundle {
            mesh: nebula_mesh,
            material: nebula_material,
            transform: Transform::from_translation(position),
            ..default()
        });
    }
}

/// Setup camera effects for better space atmosphere
fn setup_camera_effects(
    mut commands: Commands,
    camera_query: Query<Entity, With<Camera3d>>,
) {
    if let Ok(camera_entity) = camera_query.get_single() {
        // Add bloom effect for bright objects (stars, sun) — tuned for subtle, realistic corona
        commands.entity(camera_entity).insert((
            BloomSettings {
                intensity: 0.25, // Slightly increased intensity for better visible glow
                low_frequency_boost: 0.6, // Broader soft glow
                low_frequency_boost_curvature: 0.4,
                high_pass_frequency: 0.1, // Allow lower frequencies to bloom (more large glow)
                prefilter_settings: bevy::core_pipeline::bloom::BloomPrefilterSettings {
                    threshold: 2.0, // Lower threshold so our glow materials (brightness ~5-10) trigger bloom
                    threshold_softness: 0.3,
                },
                composite_mode: bevy::core_pipeline::bloom::BloomCompositeMode::Additive,
            },
            Tonemapping::ReinhardLuminance, // Better for handling extreme dynamic range
        ));
    }
}
