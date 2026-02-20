// star_halo_3d.wgsl — Outer soft glow shell for stellar bodies
//
// Renders on a translucent sphere at ~6× star radius.  Provides the
// wide, diffuse glow that makes the star visible at distance.  Uses
// a combination of Gaussian radial falloff and gentle view-dependent
// limb-brightening with coarse 3D FBM for subtle structural variation.
//
// This is deliberately cheaper than the inner corona (no ray-march).
// Star centre is derived analytically from fragment geometry.

#import bevy_pbr::mesh_view_bindings::view

@group(3) @binding(0) var<uniform> color_halo:  vec4<f32>;
@group(3) @binding(1) var<uniform> time_phase:  f32;
// halo_params.x = star surface radius, .y = halo shell radius
@group(3) @binding(2) var<uniform> halo_params: vec4<f32>;

const PI: f32 = 3.14159265359;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal:   vec3<f32>,
    @location(2) uv:             vec2<f32>,
};

// ── Noise primitives ──────────────────────────────────────────────────────

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
    q += vec3<f32>(dot(q, q.yzx + 33.33));
    return fract((q.x + q.y) * q.z);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(
            mix(hash31(i + vec3(0.0, 0.0, 0.0)), hash31(i + vec3(1.0, 0.0, 0.0)), u.x),
            mix(hash31(i + vec3(0.0, 1.0, 0.0)), hash31(i + vec3(1.0, 1.0, 0.0)), u.x),
            u.y
        ),
        mix(
            mix(hash31(i + vec3(0.0, 0.0, 1.0)), hash31(i + vec3(1.0, 0.0, 1.0)), u.x),
            mix(hash31(i + vec3(0.0, 1.0, 1.0)), hash31(i + vec3(1.0, 1.0, 1.0)), u.x),
            u.y
        ),
        u.z
    );
}

fn fbm2(p: vec3<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.6;
    var pos = p;
    for (var i = 0; i < 2; i++) {
        val += amp * noise3(pos);
        pos  = pos * 2.03 + vec3<f32>(1.7, 3.9, 2.3);
        amp *= 0.4;
    }
    return val;
}

// ── Main fragment ─────────────────────────────────────────────────────────

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let star_r  = halo_params.x;
    let shell_r = halo_params.y;

    // Derive star centre from fragment geometry
    let center = in.world_position.xyz - normalize(in.world_normal) * shell_r;

    let cam_pos  = view.world_position.xyz;
    let view_dir = normalize(cam_pos - in.world_position.xyz);
    let N        = normalize(in.world_normal);

    // Distance from star centre, normalised:  0 at star surface, 1 at shell edge
    let dist_from_center = length(in.world_position.xyz - center);
    let altitude = clamp((dist_from_center - star_r) / (shell_r - star_r), 0.0, 1.0);

    // ── Gaussian radial glow ──────────────────────────────────────
    // Strong near the star, fading smoothly to zero at the shell edge.
    // This is the main glow — not a hard ring.
    let glow = exp(-altitude * altitude * 4.5) * (1.0 - altitude);

    // ── Gentle limb-brightening ───────────────────────────────────
    // Add slight edge brightening for a sense of volume, but keep it
    // subtle — the glow should be brightest near the star centre, not
    // at the silhouette edge.
    let ndotv = abs(dot(N, view_dir));
    let limb  = 1.0 + pow(1.0 - ndotv, 3.0) * 0.4;

    // ── Structural variation (streamers) ──────────────────────────
    let norm_pos  = (in.world_position.xyz - center) / star_r;
    let t_slow    = time_phase * 0.04;
    let streamer  = fbm2(norm_pos * 2.0 + vec3<f32>(t_slow, -t_slow * 0.5, t_slow * 0.3));
    let structure = 0.7 + streamer * 0.6;

    // ── Combine ───────────────────────────────────────────────────
    let intensity = glow * limb * structure;

    // HDR output for bloom
    let hdr_out = color_halo.rgb * intensity * 2.5;

    // Alpha: fade smoothly, no hard edge
    let alpha = clamp(intensity * 1.8, 0.0, 0.85);

    return vec4<f32>(hdr_out, alpha);
}
