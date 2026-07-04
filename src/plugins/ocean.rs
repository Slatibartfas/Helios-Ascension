use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

use crate::astronomy::components::{OceanProperties, OceanType};
use crate::plugins::solar_system::CelestialBody;
use crate::plugins::solar_system_data::BodyType;

/// Plugin that registers the ocean surface material and reactive spawn/update systems.
pub struct OceanPlugin;

impl Plugin for OceanPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<OceanMaterial>::default())
            .add_systems(Update, (spawn_ocean_shell_reactive, update_ocean_shell));
    }
}

/// Marker inserted on a body entity once its ocean shell has been spawned.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct HasOceanShell;

/// Marker component for the ocean shell child entity.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct OceanShell {
    pub body_entity: Entity,
}

/// Custom material for ocean surface rendering.
///
/// Applied to a translucent sphere at 1.001× the planet's visual radius,
/// sitting between the surface and the cloud deck.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct OceanMaterial {
    /// Base colour of the ocean. .a = overall opacity.
    #[uniform(0)]
    pub ocean_color: Vec4,

    /// Fresnel parameters.
    /// .x = bias, .y = scale, .z = power, .w = specular intensity
    #[uniform(1)]
    pub ocean_params: Vec4,
}

impl Material for OceanMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/ocean.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/ocean.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        // Ocean sits between surface (0.0) and atmosphere (1.0)
        0.3
    }
}

impl OceanMaterial {
    /// Build an ocean material from an `OceanProperties` component.
    pub fn from_properties(ocean: &OceanProperties) -> Self {
        let (r, g, b, opacity) = match ocean.ocean_type {
            OceanType::Water => (0.02, 0.08, 0.35, 0.85),
            OceanType::Methane => (0.15, 0.12, 0.05, 0.70),
            OceanType::Hydrocarbon => (0.2, 0.15, 0.05, 0.65),
            OceanType::Ammonia => (0.25, 0.22, 0.35, 0.75),
            OceanType::Subsurface => (0.05, 0.12, 0.3, 0.40), // Faint hint through ice
        };

        // Scale opacity by surface fraction so partial oceans are thinner
        let effective_opacity = opacity * ocean.surface_fraction;

        Self {
            ocean_color: Vec4::new(r, g, b, effective_opacity),
            ocean_params: Vec4::new(
                0.02,                                        // Fresnel bias
                1.0,                                         // Fresnel scale
                3.0,                                         // Fresnel power
                if ocean.is_subsurface { 0.1 } else { 0.8 }, // specular intensity
            ),
        }
    }
}

/// Reactively spawns an ocean shell when a body gains `OceanProperties`.
fn spawn_ocean_shell_reactive(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OceanMaterial>>,
    query: Query<
        (Entity, &OceanProperties, &CelestialBody),
        (Added<OceanProperties>, Without<HasOceanShell>),
    >,
) {
    for (entity, ocean, body) in &query {
        if body.body_type == BodyType::Star || body.body_type == BodyType::GasGiant {
            continue;
        }
        // Don't render a visible shell for subsurface oceans — they're under ice
        if ocean.is_subsurface {
            commands.entity(entity).insert(HasOceanShell);
            continue;
        }

        let ocean_radius = body.visual_radius * 1.001;
        let mat = OceanMaterial::from_properties(ocean);

        commands
            .entity(entity)
            .insert(HasOceanShell)
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Sphere::new(ocean_radius).mesh().uv(64, 32))),
                    MeshMaterial3d(materials.add(mat)),
                    Transform::default(),
                    OceanShell {
                        body_entity: entity,
                    },
                ));
            });
    }
}

/// Updates the ocean shell material when `OceanProperties` changes
/// (e.g. through terraforming).
fn update_ocean_shell(
    mut materials: ResMut<Assets<OceanMaterial>>,
    changed_bodies: Query<
        (Entity, &OceanProperties, Option<&Children>),
        (Changed<OceanProperties>, With<HasOceanShell>),
    >,
    shells: Query<(&OceanShell, &MeshMaterial3d<OceanMaterial>)>,
) {
    for (entity, ocean, maybe_children) in &changed_bodies {
        let Some(children) = maybe_children else {
            continue;
        };
        for child in children.iter() {
            if let Ok((shell, mat_handle)) = shells.get(child) {
                if shell.body_entity == entity {
                    if let Some(mat) = materials.get_mut(&mat_handle.0) {
                        *mat = OceanMaterial::from_properties(ocean);
                    }
                }
            }
        }
    }
}
