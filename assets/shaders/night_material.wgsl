#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings
#import bevy_pbr::mesh_functions

@group(2) @binding(0) var night_texture: texture_2d<f32>;
@group(2) @binding(1) var night_sampler: sampler;
@group(2) @binding(2) var<uniform> sun_position: vec4<f32>; // .xyz is position, .w is unused

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Sample night texture
    let night_color = textureSample(night_texture, night_sampler, in.uv);
    
    // Calculate vector to sun
    // Assumes sun is at (0,0,0) usually, but we make it configurable via uniform
    let to_sun = normalize(sun_position.xyz - in.world_position.xyz);
    
    // Normalize normal
    let normal = normalize(in.world_normal);
    
    // Calculate N dot L (diffuse lighting factor)
    let ndotl = dot(normal, to_sun);
    
    // smoothstep for soft transition at terminator
    // We want visibility when ndotl < 0 (night side)
    // Transition from 0.2 (day/twilight starts masking) to -0.2 (full night)
    let visibility = 1.0 - smoothstep(-0.2, 0.2, ndotl);
    
    // Return color * visibility
    // Alpha is 1.0 because we use One means One blending (additive) or we can use alpha if we set BlendMode
    // Using Additive blending: output color is added to buffer.
    
    return night_color * visibility;
}
