
use bevy::prelude::*;
use bevy::ecs::system::SystemParam;

// Test: what does Query<'w, Data> expand to?
// If it needs 2 lifetimes, this should fail
fn _takes_query<Q>(_: Q) where Q: SystemParam {}

fn test_fn() {
    // Try creating a query with just 2 type params (shorthand)
    // If Query needs 3 params (world, state, data), this won't work
    let _: Query<'static, Entity>;
}
