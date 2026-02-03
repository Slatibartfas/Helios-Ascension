use bevy::prelude::*;

// Test that the app can be created without errors
#[test]
fn test_app_creation() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    // Add a simple system to verify app runs
    app.add_systems(Startup, || {
        println!("Test app started successfully");
    });

    // Run a single update cycle
    app.update();
}

// Test that basic ECS operations work
#[test]
fn test_ecs_basics() {
    let mut world = World::new();

    // Create an entity with a transform
    let entity = world.spawn(Transform::from_xyz(1.0, 2.0, 3.0)).id();

    // Query for the entity
    let transform = world.get::<Transform>(entity).unwrap();
    assert_eq!(transform.translation.x, 1.0);
    assert_eq!(transform.translation.y, 2.0);
    assert_eq!(transform.translation.z, 3.0);
}

// Test that we can create multiple entities
#[test]
fn test_multiple_entities() {
    let mut world = World::new();

    // Create multiple entities
    world.spawn(Transform::from_xyz(0.0, 0.0, 0.0));
    world.spawn(Transform::from_xyz(1.0, 1.0, 1.0));
    world.spawn(Transform::from_xyz(2.0, 2.0, 2.0));

    // Count entities with Transform component
    let mut query = world.query::<&Transform>();
    let count = query.iter(&world).count();
    assert_eq!(count, 3);
}
