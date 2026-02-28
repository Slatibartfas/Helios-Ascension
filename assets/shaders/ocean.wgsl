// Ocean surface shader — Fresnel reflection + depth-based tint.
//
// Renders on a translucent sphere at 1.001× planet visual radius.
// Uses a simple Fresnel term to blend between a deep-water colour and
// a specular reflection of the sky/environment.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_bindings
#import bevy_pbr::mesh_functions

// ocean_color.rgb = base tint of the liquid, .a = overall opacity
@group(3) @binding(0) var<uniform> ocean_color: vec4<f32>;
// ocean_params.x = Fresnel bias, .y = Fresnel scale, .z = Fresnel power, .w = specular intensity
@group(3) @binding(1) var<uniform> ocean_params: vec4<f32>;

const PI: f32 = 3.14159265359;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
}

@vertex
fn vertex(
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
) -> VertexOutput {
    var out: VertexOutput;

    let model = mesh_functions::get_world_from_local(instance_index);
    let world_pos = model * vec4<f32>(position, 1.0);
    out.world_position = world_pos;
    out.world_normal = normalize((model * vec4<f32>(normal, 0.0)).xyz);
    out.clip_position = view.clip_from_world * world_pos;
    return out;
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let V = normalize(view.world_position.xyz - in.world_position.xyz);
    let NdotV = max(dot(N, V), 0.0);

    // Schlick-style Fresnel approximation
    let fresnel_bias  = ocean_params.x;
    let fresnel_scale = ocean_params.y;
    let fresnel_power = ocean_params.z;
    let specular_intensity = ocean_params.w;

    let fresnel = fresnel_bias + fresnel_scale * pow(1.0 - NdotV, fresnel_power);

    // Base ocean colour (deep water tint)
    let deep_color = ocean_color.rgb;

    // Simple sky reflection — approximate as a bright pale colour
    let sky_color = vec3<f32>(0.6, 0.75, 0.95);

    // Mix deep water with sky reflection based on Fresnel
    let surface = mix(deep_color, sky_color, clamp(fresnel, 0.0, 1.0));

    // Simple sun specular highlight (assume sun roughly at +Z for now — 
    // the planet rotates so this gives a moving glint effect)
    let L = normalize(vec3<f32>(0.0, 0.0, 1.0) - in.world_position.xyz);
    let H = normalize(V + L);
    let spec = pow(max(dot(N, H), 0.0), 64.0) * specular_intensity;

    let final_color = surface + vec3<f32>(spec);
    let alpha = ocean_color.a * (0.5 + 0.5 * fresnel); // More transparent when looking straight down

    return vec4<f32>(final_color, clamp(alpha, 0.0, 0.95));
}
