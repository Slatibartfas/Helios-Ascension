use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};
use crate::plugins::visual_effects::StarParticle;

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GridMaterial>::default())
            .add_systems(Startup, setup_grid)
            .add_systems(Update, (spawn_droplines, update_droplines));
    }
}

/// Material for the tactical grid on the ecliptic plane
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GridMaterial {
    #[uniform(0)]
    pub grid_params: GridParams,
}

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct GridParams {
    pub grid_scale: f32,
    pub fade_distance: f32,
    pub max_distance: f32,
    pub alpha: f32,
}

impl Material for GridMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/grid_material.wgsl".into()
    }
    
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

/// Component marker for the tactical grid plane
#[derive(Component)]
pub struct TacticalGrid;

/// Component marker for vertical droplines
#[derive(Component)]
pub struct Dropline {
    /// Entity this dropline is attached to
    pub parent_entity: Entity,
}

/// Marker component to indicate an entity already has a dropline
#[derive(Component)]
pub struct HasDropline;

/// Setup the ecliptic grid plane
fn setup_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GridMaterial>>,
) {
    // Create a large horizontal plane for the grid
    // The plane is in the XZ plane (Y=0), which represents the ecliptic
    let grid_size = 100000.0; // Large enough to cover the solar system view
    let grid_mesh = meshes.add(Plane3d::default().mesh().size(grid_size, grid_size));
    
    let grid_material = materials.add(GridMaterial {
        grid_params: GridParams {
            grid_scale: 1000.0,      // Grid cell size in game units (1000 = ~6.68 AU if 1 unit = 1 million km)
            fade_distance: 20000.0,  // Start fading at this distance from center
            max_distance: 50000.0,   // Fully transparent at this distance
            alpha: 0.1,              // Base transparency (very subtle)
        },
    });
    
    commands.spawn((
        MaterialMeshBundle {
            mesh: grid_mesh,
            material: grid_material,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        },
        TacticalGrid,
    ));
}

/// Spawn droplines for entities that have a Transform but don't have a dropline yet
/// This system uses Changed<Transform> to only process newly added entities
fn spawn_droplines(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    // Query entities that were just added or changed and need droplines
    entities_query: Query<
        Entity,
        (
            With<Transform>,
            Without<HasDropline>,
            Without<Dropline>,
            Without<Camera>,
            Without<TacticalGrid>,
            Without<StarParticle>,
        ),
    >,
) {
    if entities_query.is_empty() {
        return;
    }
    
    // Create a thin cylinder mesh for droplines (shared)
    let cylinder_mesh = meshes.add(Cylinder::new(0.5, 1.0));
    
    // Subtle cyan material for droplines (shared)
    let dropline_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.6, 0.8, 0.3),
        emissive: LinearRgba::rgb(0.1, 0.3, 0.4),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    
    for entity in entities_query.iter() {
        // Spawn dropline for this entity
        commands.spawn((
            PbrBundle {
                mesh: cylinder_mesh.clone(),
                material: dropline_material.clone(),
                transform: Transform::default(), // Will be updated in update_droplines
                ..default()
            },
            Dropline {
                parent_entity: entity,
            },
        ));
        
        // Mark the parent entity as having a dropline to avoid duplicate creation
        commands.entity(entity).insert(HasDropline);
    }
}

/// Update dropline positions to follow their parent entities
fn update_droplines(
    mut droplines_query: Query<(&Dropline, &mut Transform)>,
    entities_query: Query<&GlobalTransform, Without<Dropline>>,
) {
    for (dropline, mut transform) in droplines_query.iter_mut() {
        if let Ok(parent_transform) = entities_query.get(dropline.parent_entity) {
            let parent_pos = parent_transform.translation();
            
            // Position the dropline from the parent entity down to Y=0 (ecliptic plane)
            let height = parent_pos.y.abs();
            let midpoint_y = parent_pos.y / 2.0;
            
            // Scale the cylinder to reach from object to plane
            // Cylinder has default height of 1.0, so scale by the actual height needed
            transform.scale = Vec3::new(1.0, height, 1.0);
            
            // Position at the midpoint between object and plane
            transform.translation = Vec3::new(parent_pos.x, midpoint_y, parent_pos.z);
            
            // No rotation needed - cylinder is already vertical (along Y axis)
        }
    }
}
