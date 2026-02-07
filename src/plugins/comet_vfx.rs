use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use crate::plugins::solar_system::{Comet, Star};
use crate::astronomy::components::CometTail; // Reuse existing marker if possible, or define our own for particles

/// Plugin for hyper-realistic comet tail effects
pub struct CometVfxPlugin;

impl Plugin for CometVfxPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HanabiPlugin>() {
            app.add_plugins(HanabiPlugin);
        }
        
        app.insert_resource(CometTailResources::default())
            .add_systems(Startup, setup_comet_effects)
            .add_systems(Update, (
                attach_comet_tails,
                update_comet_vectors,
                cleanup_legacy_tails,
            ));
    }
}

/// Cleanup any legacy mesh-based tails that might have been spawned by old systems
fn cleanup_legacy_tails(
    mut commands: Commands,
    query: Query<Entity, With<CometTail>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

#[derive(Resource, Default)]
struct CometTailResources {
    ion_tail: Handle<EffectAsset>,
    dust_tail: Handle<EffectAsset>,
}

#[derive(Component)]
struct CometVfxController {
    last_position: Vec3,
    velocity: Vec3,
}

/// Setup the Hanabi effect assets for Ion and Dust tails
fn setup_comet_effects(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
    mut resources: ResMut<CometTailResources>,
) {
    // -----------------------------------------------------------------------
    // 1. The Ion Tail Effect (Type I)
    // -----------------------------------------------------------------------
    // - Thin, high-velocity particles
    // - Blue-to-transparent gradient
    // - Additive blending (done via EffectBundle configuration later, or Material)
    // - Physics: "SolarWind" acceleration away from Sun
    
    // Properties are now defined on the writer before building the graph
    // (Old lines removed)
    
    // Color Gradient: Bright Cyan/Blue -> Transparent (HDR Intensity for Bloom)
    let mut ion_gradient = Gradient::new();
    ion_gradient.add_key(0.0, Vec4::new(2.0, 4.0, 10.0, 1.0)); // Ultra-bright Blue core
    ion_gradient.add_key(0.2, Vec4::new(1.0, 2.0, 5.0, 0.8)); // Bright blue
    ion_gradient.add_key(1.0, Vec4::new(0.0, 0.2, 1.0, 0.0)); // Fade out

    let mut writer = ExprWriter::new();
    
    // Define properties to be updated every frame
    let solar_wind_prop = writer.add_property("solar_wind_force", (Vec3::Y * 50.0).into()); // Vector3: Direction * Strength
    let comet_vel_prop = writer.add_property("comet_velocity", Vec3::ZERO.into());

    // Init: Spawn at parent position
    let init_pos = SetAttributeModifier::new(
        Attribute::POSITION,
        writer.lit(Vec3::ZERO).expr(),
    );
    
    // Init: Lifetime (randomized)
    let init_age = SetAttributeModifier::new(
        Attribute::AGE,
        writer.lit(0.0).expr(),
    );
    let init_lifetime = SetAttributeModifier::new(
        Attribute::LIFETIME,
        writer.lit(2.0).uniform(writer.lit(3.0)).expr(),
    );

    // Init: High initial velocity to shoot out, or just let the wind take it?
    // "High-velocity particles". Let's give them some initial push plus the wind.
    // The wind dominates.
    
    let particle_size = SetAttributeModifier::new(
        Attribute::SIZE,
        writer.lit(0.5).expr(), 
    );

    // Init: Velocity = Comet Velocity (Type I follows the nucleus closely but is pushed by wind)
    let init_vel_ion = SetAttributeModifier::new(
        Attribute::VELOCITY,
        writer.prop(comet_vel_prop).expr(),
    );

    // Update: Apply Solar Wind Force (Acceleration)
    // Force = Property("solar_wind_force")
    let update_accel = AccelModifier::new(
        writer.prop(solar_wind_prop).expr()
    );

    let ion_effect = EffectAsset::new(
        vec![4096], // Capacity
        Spawner::rate(200.0.into()),
        writer.finish(),
    )
    .with_name("ion_tail")
    .with_simulation_space(SimulationSpace::Global)
    .init(init_pos)
    .init(init_age)
    .init(init_lifetime)
    .init(particle_size)
    .init(init_vel_ion)
    .update(update_accel)
    .render(ColorOverLifetimeModifier { gradient: ion_gradient })
    .render(SizeOverLifetimeModifier { 
        gradient: Gradient::constant(Vec2::splat(1.0)), 
        screen_space_size: false 
    });

    // -----------------------------------------------------------------------
    // 2. The Dust Tail Effect (Type II)
    // -----------------------------------------------------------------------
    // - Warm white/yellowish
    // - Diffuse, varied sizes
    // - Inherit 90% comet velocity
    // - Radiation pressure (weaker away from sun)
    // - Drag/Damping
    
    // Properties are now defined on writer_dust
    
    let mut dust_gradient = Gradient::new();
    dust_gradient.add_key(0.0, Vec4::new(1.5, 1.35, 1.2, 1.0)); // HDR Reflective White/Yellow
    dust_gradient.add_key(1.0, Vec4::new(0.8, 0.7, 0.5, 0.0)); // Fade to dusty brown/transparent

    let mut writer_dust = ExprWriter::new();
    let comet_vel_prop = writer_dust.add_property("comet_velocity", Vec3::ZERO.into());
    let radiation_pressure_prop = writer_dust.add_property("radiation_pressure", (Vec3::Y * 2.0).into()); // Vector3

    // Init: Position
    let init_pos_dust = SetAttributeModifier::new(
        Attribute::POSITION,
        writer_dust.lit(Vec3::ZERO).expr(),
    );
    
    // Init: Velocity = Comet Velocity * 0.99 (Almost matches nucleus, allowing wind to curve it naturally)
    // Need to access property 'comet_velocity' in Init
    let init_vel_dust = SetAttributeModifier::new(
        Attribute::VELOCITY,
        writer_dust.prop(comet_vel_prop).mul(writer_dust.lit(0.99)).expr(),
    );
    
    let init_lifetime_dust = SetAttributeModifier::new(
        Attribute::LIFETIME,
        writer_dust.lit(4.0).uniform(writer_dust.lit(6.0)).expr(),
    );

    // Init: Varied sizes
    let init_size_dust = SetAttributeModifier::new(
        Attribute::SIZE,
        writer_dust.lit(0.8).uniform(writer_dust.lit(2.5)).expr(),
    );

    // Update: Radiation Pressure (Acceleration)
    let update_rad_pressure = AccelModifier::new(
        writer_dust.prop(radiation_pressure_prop).expr()
    );

    // Update: Drag
    let update_drag = LinearDragModifier::new(
        writer_dust.lit(0.5).expr() // Drag coefficient
    );

    let dust_effect = EffectAsset::new(
        vec![8192],
        Spawner::rate(300.0.into()),
        writer_dust.finish(),
    )
    .with_name("dust_tail")
    .with_simulation_space(SimulationSpace::Global)
    .init(init_pos_dust)
    .init(init_vel_dust)
    .init(init_lifetime_dust)
    .init(init_size_dust)
    .update(update_rad_pressure)
    .update(update_drag)
    .render(ColorOverLifetimeModifier { gradient: dust_gradient })
    .render(SizeOverLifetimeModifier { 
        gradient: Gradient::constant(Vec2::splat(1.0)), 
        screen_space_size: false 
    });

    resources.ion_tail = effects.add(ion_effect);
    resources.dust_tail = effects.add(dust_effect);
}

/// Spawns particle effects for new comets
fn attach_comet_tails(
    mut commands: Commands,
    query: Query<(Entity, &Transform), (With<Comet>, Without<CometVfxController>)>,
    resources: Res<CometTailResources>,
) {
    for (entity, transform) in query.iter() {
        // Skip comets that are still likely at the world origin (uninitialized position)
        // This prevents tails from spawning at (0,0,0) and shooting out when the orbital position updates
        if transform.translation.length_squared() < 0.1 {
            continue;
        }

        // Add controller to parent
        commands.entity(entity).insert(CometVfxController {
            last_position: transform.translation,
            velocity: Vec3::ZERO,
        });

        // Ion Tail
        commands.spawn(ParticleEffectBundle {
            effect: ParticleEffect::new(resources.ion_tail.clone()),
            transform: Transform::IDENTITY, 
            ..default()
        })
        .insert(Name::new("Ion Tail"))
        .set_parent(entity);

        // Dust Tail
        commands.spawn(ParticleEffectBundle {
            effect: ParticleEffect::new(resources.dust_tail.clone()),
            transform: Transform::IDENTITY,
            ..default()
        })
        .insert(Name::new("Dust Tail"))
        .set_parent(entity);
    }
}

/// Updates the physics vectors for the tails based on Sun position
fn update_comet_vectors(
    time: Res<Time>,
    mut comet_query: Query<(&GlobalTransform, &Children, &mut CometVfxController), With<Comet>>,
    mut effect_query: Query<(&mut EffectProperties, &Name)>,
    star_query: Query<&GlobalTransform, With<Star>>,
) {
    // Assume primary star (Sun) is the first/only star for now
    let sun_pos = if let Ok(star_transform) = star_query.get_single() {
        star_transform.translation()
    } else {
        return; // No sun found
    };

    let dt = time.delta_seconds();
    if dt == 0.0 { return; }

    for (comet_transform, children, mut controller) in comet_query.iter_mut() {
        let comet_pos = comet_transform.translation();
        
        // Handle potential teleportation from origin (if the check in attach_comet_tails wasn't enough)
        // If last position was at origin and now we are far away, it's an initialization jump.
        // Reset last_position without calculating a massive velocity.
        if controller.last_position.length_squared() < 0.1 && comet_pos.length_squared() > 10.0 {
            controller.last_position = comet_pos;
            controller.velocity = Vec3::ZERO;
            continue;
        }

        // Calculate Velocity (for Dust Tail)
        let current_velocity = (comet_pos - controller.last_position) / dt;
        controller.velocity = current_velocity;
        controller.last_position = comet_pos;

        // Calculate Solar Wind Direction (Away from Sun)
        let to_comet = comet_pos - sun_pos;
        let distance = to_comet.length();
        let dir_away_from_sun = to_comet.normalize_or_zero();

        // Strength falls off with distance? Or constant "wind speed"? 
        // Prompt says "Apply a ... acceleration vector". 
        // Real solar wind drops off, but let's keep it simple or visual.
        // Let's make it strong enough to be visible.
        let solar_wind_strength = 20.0; // Tunable
        let solar_wind_vector = dir_away_from_sun * solar_wind_strength;

        // Radiation Pressure: 5x weaker than Solar Wind
        let radiation_pressure_vector = solar_wind_vector * 0.2;

        // Update Effects
        for &child in children.iter() {
            if let Ok((mut properties, name)) = effect_query.get_mut(child) {
                if name.as_str() == "Ion Tail" {
                    // Ion tail needs Solar Wind Vector AND Comet Velocity
                    properties.set("solar_wind_force", solar_wind_vector.into());
                    properties.set("comet_velocity", current_velocity.into());
                } else if name.as_str() == "Dust Tail" {
                    // Dust tail needs Comet Velocity and Radiation Pressure
                    properties.set("comet_velocity", current_velocity.into());
                    properties.set("radiation_pressure", radiation_pressure_vector.into());
                }
            }
        }
    }
}
