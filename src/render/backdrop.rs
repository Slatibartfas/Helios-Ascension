use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
};
use bevy::render::view::RenderLayers;

// Backdrop configuration constants
// Keep backdrop radius below camera far plane (1_500_000.0) to avoid clipping
const BACKDROP_SPHERE_RADIUS: f32 = 1_400_000.0;
const BACKDROP_SPHERE_UV_SEGMENTS: usize = 32;
const BACKDROP_SPHERE_UV_RINGS: usize = 18;

/// Plugin that manages the procedural space backdrop
pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SkyboxMaterial>::default())
            .add_systems(Startup, spawn_backdrop_sphere)
            .add_systems(PostUpdate, update_backdrop_position);
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

    // Override specialize to disable depth write and enable back-face rendering
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

        // Cull front faces instead of back faces so we can see the inside of the sphere
        descriptor.primitive.cull_mode = Some(Face::Front);

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
            .uv(BACKDROP_SPHERE_UV_SEGMENTS, BACKDROP_SPHERE_UV_RINGS),
    );

    let backdrop_material = materials.add(SkyboxMaterial::default());

    info!(
        "Spawning procedural space backdrop sphere with radius {}",
        BACKDROP_SPHERE_RADIUS
    );

    commands.spawn((
        MaterialMeshBundle {
            mesh: backdrop_mesh,
            material: backdrop_material,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        },
        BackdropSphere,
        RenderLayers::layer(0), // Backdrop is on default render layer (layer 0)
    ));
}

/// Update backdrop sphere to always center on the camera
fn update_backdrop_position(
    mut backdrop_query: Query<(&mut Transform, &Handle<SkyboxMaterial>), With<BackdropSphere>>,
    camera_query: Query<
        (&Transform, &crate::plugins::camera::OrbitCamera),
        (With<Camera3d>, Without<BackdropSphere>),
    >,
    mut materials: ResMut<Assets<SkyboxMaterial>>,
) {
    if let (Ok((mut backdrop_transform, material_handle)), Ok((camera_transform, orbit_camera))) =
        (backdrop_query.get_single_mut(), camera_query.get_single())
    {
        // Center backdrop on camera position
        backdrop_transform.translation = camera_transform.translation;

        // Update material uniforms for parallax and distance fading
        if let Some(material) = materials.get_mut(material_handle) {
            // Extract rotation from camera transform
            material.camera_rotation = Mat3::from_quat(camera_transform.rotation);

            // Calculate normalized camera distance from orbit camera zoom level
            // This properly tracks zoom rather than world position
            material.camera_distance = ((orbit_camera.radius - orbit_camera.min_radius)
                / (orbit_camera.max_radius - orbit_camera.min_radius))
                .clamp(0.0, 1.0);
        }
    }
}
