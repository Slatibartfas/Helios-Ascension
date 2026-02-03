use bevy::prelude::*;

pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_solar_system)
            .add_systems(Update, (update_orbits, rotate_bodies));
    }
}

#[derive(Component)]
pub struct CelestialBody {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub radius: f32,
    #[allow(dead_code)]
    pub mass: f32,
}

#[derive(Component)]
pub struct Star;

#[derive(Component)]
pub struct Planet;

#[derive(Component)]
pub struct OrbitalPath {
    pub parent: Option<Entity>,
    pub semi_major_axis: f32,
    #[allow(dead_code)]
    pub eccentricity: f32,
    #[allow(dead_code)]
    pub orbital_period: f32,
    pub current_angle: f32,
    pub angular_velocity: f32,
}

#[derive(Component)]
pub struct RotationSpeed(pub f32);

fn setup_solar_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Create the Sun
    let sun = commands
        .spawn((
            PbrBundle {
                mesh: meshes.add(Sphere::new(5.0)),
                material: materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 0.9, 0.3),
                    emissive: Color::srgb(10.0, 8.0, 2.0).into(),
                    ..default()
                }),
                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                ..default()
            },
            CelestialBody {
                name: "Sol".to_string(),
                radius: 5.0,
                mass: 1.989e30,
            },
            Star,
            RotationSpeed(0.1),
        ))
        .id();

    // Add a point light at the sun's position
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 5000000.0,
            range: 1000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..default()
    });

    // Create Mercury
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(0.8)),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.7, 0.7, 0.7),
                ..default()
            }),
            transform: Transform::from_xyz(15.0, 0.0, 0.0),
            ..default()
        },
        CelestialBody {
            name: "Mercury".to_string(),
            radius: 0.8,
            mass: 3.285e23,
        },
        Planet,
        OrbitalPath {
            parent: Some(sun),
            semi_major_axis: 15.0,
            eccentricity: 0.0,
            orbital_period: 88.0,
            current_angle: 0.0,
            angular_velocity: 2.0 * std::f32::consts::PI / 88.0,
        },
        RotationSpeed(0.5),
    ));

    // Create Venus
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(1.5)),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.9, 0.8, 0.5),
                ..default()
            }),
            transform: Transform::from_xyz(25.0, 0.0, 0.0),
            ..default()
        },
        CelestialBody {
            name: "Venus".to_string(),
            radius: 1.5,
            mass: 4.867e24,
        },
        Planet,
        OrbitalPath {
            parent: Some(sun),
            semi_major_axis: 25.0,
            eccentricity: 0.0,
            orbital_period: 225.0,
            current_angle: 0.7,
            angular_velocity: 2.0 * std::f32::consts::PI / 225.0,
        },
        RotationSpeed(0.3),
    ));

    // Create Earth
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(1.6)),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.4, 0.8),
                ..default()
            }),
            transform: Transform::from_xyz(35.0, 0.0, 0.0),
            ..default()
        },
        CelestialBody {
            name: "Earth".to_string(),
            radius: 1.6,
            mass: 5.972e24,
        },
        Planet,
        OrbitalPath {
            parent: Some(sun),
            semi_major_axis: 35.0,
            eccentricity: 0.0,
            orbital_period: 365.0,
            current_angle: 1.5,
            angular_velocity: 2.0 * std::f32::consts::PI / 365.0,
        },
        RotationSpeed(0.4),
    ));

    // Create Mars
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(1.2)),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.3, 0.2),
                ..default()
            }),
            transform: Transform::from_xyz(50.0, 0.0, 0.0),
            ..default()
        },
        CelestialBody {
            name: "Mars".to_string(),
            radius: 1.2,
            mass: 6.39e23,
        },
        Planet,
        OrbitalPath {
            parent: Some(sun),
            semi_major_axis: 50.0,
            eccentricity: 0.0,
            orbital_period: 687.0,
            current_angle: 3.0,
            angular_velocity: 2.0 * std::f32::consts::PI / 687.0,
        },
        RotationSpeed(0.45),
    ));

    // Create Jupiter
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(4.0)),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.7, 0.5),
                ..default()
            }),
            transform: Transform::from_xyz(75.0, 0.0, 0.0),
            ..default()
        },
        CelestialBody {
            name: "Jupiter".to_string(),
            radius: 4.0,
            mass: 1.898e27,
        },
        Planet,
        OrbitalPath {
            parent: Some(sun),
            semi_major_axis: 75.0,
            eccentricity: 0.0,
            orbital_period: 4333.0,
            current_angle: 4.2,
            angular_velocity: 2.0 * std::f32::consts::PI / 4333.0,
        },
        RotationSpeed(0.7),
    ));
}

fn update_orbits(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut OrbitalPath)>,
    parent_query: Query<&Transform, Without<OrbitalPath>>,
) {
    for (mut transform, mut orbit) in query.iter_mut() {
        // Update the angle based on angular velocity
        orbit.current_angle += orbit.angular_velocity * time.delta_seconds();

        // Keep angle in range [0, 2π]
        if orbit.current_angle > 2.0 * std::f32::consts::PI {
            orbit.current_angle -= 2.0 * std::f32::consts::PI;
        }

        // Calculate position based on orbital parameters
        let x = orbit.semi_major_axis * orbit.current_angle.cos();
        let z = orbit.semi_major_axis * orbit.current_angle.sin();

        // Get parent position if it exists
        let parent_pos = if let Some(parent_entity) = orbit.parent {
            if let Ok(parent_transform) = parent_query.get(parent_entity) {
                parent_transform.translation
            } else {
                Vec3::ZERO
            }
        } else {
            Vec3::ZERO
        };

        transform.translation = parent_pos + Vec3::new(x, 0.0, z);
    }
}

fn rotate_bodies(time: Res<Time>, mut query: Query<(&mut Transform, &RotationSpeed)>) {
    for (mut transform, rotation_speed) in query.iter_mut() {
        transform.rotate_y(rotation_speed.0 * time.delta_seconds());
    }
}
