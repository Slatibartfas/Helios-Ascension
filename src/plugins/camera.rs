use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (camera_movement, camera_zoom));
    }
}

#[derive(Component)]
pub struct GameCamera {
    pub speed: f32,
    pub zoom_speed: f32,
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 50.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        GameCamera {
            speed: 50.0,
            zoom_speed: 10.0,
        },
    ));
}

fn camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion_events: EventReader<MouseMotion>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &GameCamera)>,
) {
    let (mut transform, camera) = query.single_mut();

    // WASD movement
    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction -= *transform.forward();
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction += *transform.forward();
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= *transform.right();
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += *transform.right();
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        direction.y += 1.0;
    }

    if direction.length() > 0.0 {
        transform.translation += direction.normalize() * camera.speed * time.delta_seconds();
    }

    // Mouse look when right button is held
    if mouse.pressed(MouseButton::Right) {
        for event in motion_events.read() {
            let delta_x = event.delta.x * 0.003;
            let delta_y = event.delta.y * 0.003;

            // Rotate around Y axis for horizontal movement
            transform.rotate_y(-delta_x);

            // Rotate around local X axis for vertical movement
            let right = transform.right();
            transform.rotate_axis(right, -delta_y);
        }
    } else {
        // Clear events if not using them
        motion_events.clear();
    }
}

fn camera_zoom(
    mut scroll_events: EventReader<MouseWheel>,
    mut query: Query<(&mut Transform, &GameCamera)>,
) {
    let (mut transform, camera) = query.single_mut();

    for event in scroll_events.read() {
        let forward = *transform.forward();
        transform.translation += forward * event.y * camera.zoom_speed;
    }
}
