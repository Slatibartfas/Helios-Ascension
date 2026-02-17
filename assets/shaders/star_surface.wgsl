// star_surface.wgsl — Physically-based limb darkening for stellar bodies
//
// Applied to the star's sphere mesh as a custom unlit material that replaces
// Bevy's StandardMaterial.  The Eddington linear limb-darkening law is used:
//
//   I(μ) = I₀ · (1 − u·(1 − μ))
//
// where μ = cos(viewing angle) = dot(N, V):
//   μ = 1.0  at disk centre   (normal faces camera directly)
//   μ = 0.0  at disk limb     (normal is edge-on)
//   u ≈ 0.62 for a G-type star in visible light
//
// An additional nonlinear power-law component sharpens the very edge of the
// disk, matching observed solar limb profiles more accurately.

#import bevy_pbr::mesh_view_bindings::view

@group(2) @binding(0) var<uniform> color_center: vec4<f32>;
@group(2) @binding(1) var<uniform> color_limb: vec4<f32>;
@group(2) @binding(2) var star_texture: texture_2d<f32>;
@group(2) @binding(3) var star_sampler: sampler;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let normal   = normalize(in.world_normal);
    let view_dir = normalize(view.world_position - in.world_position.xyz);

    // μ: cosine of angle between surface normal and viewing direction
    //    1.0 = disk centre, 0.0 = limb
    let mu = max(0.0, dot(normal, view_dir));

    // Eddington linear limb darkening (u = 0.62 for solar-like G2V star)
    let u          = 0.62;
    let ld_linear  = 1.0 - u * (1.0 - mu);

    // Extra nonlinear darkening right at the very limb for a more physical profile
    let ld_power   = pow(mu, 0.45);

    // Blend the two laws (30 % power component)
    let ld = mix(ld_linear, ld_power, 0.30);   // 1.0 at centre, ~0.38 at limb

    // Color temperature shift: centre is hot white, limb is cooler orange-red
    // Amount of shift towards limb color:
    let t_shift = clamp((1.0 - mu) * 1.6, 0.0, 1.0);
    let surface_rgb = mix(color_center.rgb, color_limb.rgb, t_shift);

    // Optional texture modulation (solar granulation, if texture loaded)
    // A white 1×1 fallback texture gives tex.r = 1.0, so tex_mod = 1.0 → no effect.
    let tex     = textureSample(star_texture, star_sampler, in.uv);
    let tex_mod = 0.78 + 0.22 * tex.r;

    // Final HDR emissive colour: bright at centre, ~38 % at limb
    let final_color = surface_rgb * ld * tex_mod;

    return vec4<f32>(final_color, 1.0);
}
