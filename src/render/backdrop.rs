use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::view::RenderLayers;

// Backdrop configuration constants
const BACKDROP_SPHERE_RADIUS: f32 = 100000.0;
const BACKDROP_SPHERE_UV_SEGMENTS: usize = 32;
const BACKDROP_SPHERE_UV_RINGS: usize = 18;
const CAMERA_MIN_RADIUS: f32 = 5.0;
const CAMERA_MAX_RADIUS: f32 = 50000.0;

/// Plugin that manages the procedural space backdrop
pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SkyboxMaterial>::default())
            .add_systems(Startup, spawn_backdrop_sphere)
            .add_systems(Update, update_backdrop_position);
    }
}

/// Component marking the backdrop sphere entity
#[derive(Component)]
pub struct BackdropSphere;

/// Material for the procedural skybox shader
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SkyboxMaterial {
    /// Camera rotation matrix for parallax calculations
    #[uniform(0)]
    pub camera_rotation: Mat3,
    
    /// Normalized camera distance (0.0 = min_radius, 1.0 = max_radius)
    /// Used for distance fading effect
    #[uniform(1)]
    pub camera_distance: f32,
}

impl Material for SkyboxMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/skybox.wgsl".into()
    }
    
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
    
    // Override specialize to disable depth write so backdrop doesn't interfere with gameplay entities
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Disable depth write - backdrop should not affect depth buffer
        if let Some(depth_stencil) = &mut descriptor.depth_stencil {
            depth_stencil.depth_write_enabled = false;
        }
        Ok(())
    }
}

impl Default for SkyboxMaterial {
    fn default() -> Self {
        Self {
            camera_rotation: Mat3::IDENTITY,
            camera_distance: 0.0,
        }
    }
}

/// Spawn the massive backdrop sphere that renders the starfield
fn spawn_backdrop_sphere(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyboxMaterial>>,
) {
    // Create a massive sphere for the backdrop
    let backdrop_mesh = meshes.add(
        Sphere::new(BACKDROP_SPHERE_RADIUS)
            .mesh()
            .uv(BACKDROP_SPHERE_UV_SEGMENTS, BACKDROP_SPHERE_UV_RINGS)
    );
    
    let backdrop_material = materials.add(SkyboxMaterial::default());
    
    info!("Spawning procedural space backdrop sphere with radius {}", BACKDROP_SPHERE_RADIUS);
    
    commands.spawn((
        MaterialMeshBundle {
            mesh: backdrop_mesh,
            material: backdrop_material,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        },
        BackdropSphere,
        // Use a render layer to ensure backdrop renders behind everything
        RenderLayers::layer(0),
    ));
}

/// Update backdrop sphere to always center on the camera
fn update_backdrop_position(
    mut backdrop_query: Query<(&mut Transform, &Handle<SkyboxMaterial>), With<BackdropSphere>>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<BackdropSphere>)>,
    mut materials: ResMut<Assets<SkyboxMaterial>>,
) {
    if let (Ok((mut backdrop_transform, material_handle)), Ok(camera_transform)) = 
        (backdrop_query.get_single_mut(), camera_query.get_single()) 
    {
        // Center backdrop on camera position
        backdrop_transform.translation = camera_transform.translation;
        
        // Update material uniforms for parallax and distance fading
        if let Some(material) = materials.get_mut(material_handle) {
            // Extract rotation from camera transform
            material.camera_rotation = Mat3::from_quat(camera_transform.rotation);
            
            // Calculate normalized camera distance for distance fading
            // This would ideally come from the camera's orbit distance
            // For now, use a simple calculation based on distance from origin
            let distance_from_origin = camera_transform.translation.length();
            
            // Normalize between min and max camera distances
            material.camera_distance = ((distance_from_origin - CAMERA_MIN_RADIUS) 
                / (CAMERA_MAX_RADIUS - CAMERA_MIN_RADIUS)).clamp(0.0, 1.0);
        }
    }
}
