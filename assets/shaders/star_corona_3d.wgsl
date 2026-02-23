// star_corona_3d.wgsl — Volumetric 3D corona shell for stellar bodies
//
// Three-layer model (matching the 2D billboard look but in full 3D):
//   1. Gaussian halo       — bright exp(-r²) core glow
//   2. FBM corona plumes   — animated 3D noise with radial extent variation
//   3. Ray spikes           — high-frequency angular FBM with radial falloff
//
// Renders on a translucent sphere at ~1.75× star radius.  The fragment shader
// ray-marches through the corona volume, but the primary visual structure
// comes from evaluating 2D-style layers using spherical coordinates
// (angle, radius from star centre) at each sample — giving the rich look
// of the original billboard with genuine 3D parallax.
//
// Star centre is derived analytically from fragment geometry.

#import bevy_pbr::mesh_view_bindings::view

@group(3) @binding(0) var<uniform> color_core:  vec4<f32>;
@group(3) @binding(1) var<uniform> color_halo:  vec4<f32>;
@group(3) @binding(2) var<uniform> time_phase:  f32;
// corona_params.x = star surface radius, .y = corona shell outer radius
@group(3) @binding(3) var<uniform> corona_params: vec4<f32>;

const PI: f32 = 3.14159265359;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal:   vec3<f32>,
    @location(2) uv:             vec2<f32>,
};

// ── Noise primitives ──────────────────────────────────────────────────────

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(127.1, 311.7));
    q += dot(q, q.yx + 33.33);
    return fract(q.x * q.y);
}

fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash21(i),                       hash21(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

fn fbm4(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 4; i++) {
        val += amp * noise2(pos);
        pos  = pos * 2.17 + vec2<f32>(3.1, 7.4);
        amp *= 0.48;
    }
    return val;
}

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

fn fbm3_3oct(p: vec3<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 3; i++) {
        val += amp * noise3(pos);
        pos  = pos * 2.13 + vec3<f32>(1.7, 3.9, 2.3);
        amp *= 0.5;
    }
    return val;
}

// ── Ray-sphere intersection ───────────────────────────────────────────────
fn ray_sphere(origin: vec3<f32>, dir: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let oc = origin - center;
    let b  = dot(oc, dir);
    let c  = dot(oc, oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return vec2<f32>(1e20, -1e20);
    }
    let sq = sqrt(disc);
    return vec2<f32>(-b - sq, -b + sq);
}

// ── Main fragment ─────────────────────────────────────────────────────────

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let star_r  = corona_params.x;
    let shell_r = corona_params.y;

    // Derive star centre from fragment geometry.
    let world_n = normalize(in.world_normal);
    let center  = in.world_position.xyz - world_n * shell_r;

    let cam_pos = view.world_position.xyz;
    let ray_dir = normalize(in.world_position.xyz - cam_pos);

    // Discard silhouette-edge fragments where the interpolated normal is nearly
    // perpendicular to the view ray — center derivation is unreliable there and
    // produces static spark artefacts at the mesh boundary.
    let ndotv = dot(-ray_dir, world_n);
    if ndotv < 0.08 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    // Intersect view ray with corona volume
    let outer_hit = ray_sphere(cam_pos, ray_dir, center, shell_r);
    if outer_hit.x > outer_hit.y {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    var t_near = max(outer_hit.x, 0.0);
    var t_far  = outer_hit.y;

    // Stop at star surface
    let inner_hit = ray_sphere(cam_pos, ray_dir, center, star_r);
    if inner_hit.x < inner_hit.y && inner_hit.x > 0.0 {
        t_far = min(t_far, inner_hit.x);
    }

    let ray_length = t_far - t_near;
    if ray_length <= 0.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // ── Ray-march: 6 samples ──────────────────────────────────────
    let NUM_STEPS = 6;
    let step_size = ray_length / f32(NUM_STEPS);
    let corona_thickness = shell_r - star_r;

    // Animation times — lively, turbulent feel
    let ct1 = time_phase * 0.50;
    let ct2 = time_phase * 0.72;
    let ct3 = time_phase * 0.35;

    var total_halo   = 0.0;
    var total_corona = 0.0;
    var total_rays   = 0.0;

    for (var i = 0; i < NUM_STEPS; i++) {
        let t = t_near + (f32(i) + 0.5) * step_size;
        let sample_pos = cam_pos + ray_dir * t;

        // Spherical coordinates relative to star centre
        let offset = sample_pos - center;
        let dist   = length(offset);
        let dir_n  = offset / dist;

        let altitude = clamp((dist - star_r) / corona_thickness, 0.0, 1.0);
        let r = 0.17 + altitude * 0.33;

        // Spherical angles
        let angle1 = atan2(dir_n.x, dir_n.z);
        let angle2 = atan2(dir_n.y, length(vec2<f32>(dir_n.x, dir_n.z)));

        // 3D perturbation — stronger for genuine turbulent parallax
        let perturb = fbm3_3oct(offset / star_r * 2.5 + vec3<f32>(ct3, -ct3 * 0.7, ct3 * 0.3)) * 0.25;
        let angle = angle1 + perturb;

        // ── Noise-modulated radial envelope (breaks roundness) ────
        // edge_warp varies widely to produce turbulent, irregular plume edges.
        // outer_fade independently forces the entire corona to zero well before
        // the shell boundary, so the edge_warp range can be wide without ever
        // producing sparkle/lightning at the geometric mesh edge.
        let edge_noise = fbm3_3oct(dir_n * 4.0 + vec3<f32>(ct1 * 0.3, ct2 * 0.2, -ct3 * 0.4));
        let edge_warp  = 0.6 + edge_noise * 0.7;  // range ~0.6..1.3 — restore turbulent edges
        // Fade drives to ~0 by altitude 0.90 so the outermost ray-march samples
        // contribute nothing regardless of noise state — no spark artefacts.
        let outer_fade = 1.0 - smoothstep(0.45, 0.90, altitude);
        let radial_env = pow(max(1.0 - r * 2.0 * edge_warp, 0.0), 1.8) * outer_fade;

        // ── Layer 1: Halo ─────────────────────────────────────────
        let halo_raw   = exp(-r * r * 55.0);
        let core_blend = smoothstep(0.125, 0.275, r);
        let halo       = halo_raw * mix(0.40, 1.0, core_blend) * 1.2;

        // ── Layer 2: Corona plumes (turbulent, high-contrast) ─────
        let fbm_a = fbm4(vec2<f32>(angle * 3.5  + ct1,         r * 7.0  - ct1 * 0.9) + perturb);
        let fbm_b = fbm4(vec2<f32>(angle * 5.0  - ct2 * 0.8,   r * 9.0  - ct2 * 1.2) - perturb);
        let fbm_c = fbm4(vec2<f32>(angle2 * 2.5 + ct3 * 0.5,   r * 5.0  - ct3 * 0.6));

        // Wider extent variation = more violent plume structure
        let extent          = 0.40 + 0.60 * fbm_c;
        let noise_raw       = 0.25 + 0.50 * fbm_a + 0.30 * fbm_b;
        // Sharpen the noise to create more defined plume edges
        let noise_intensity = pow(noise_raw, 0.8) * 1.3;
        let corona_inner    = exp(-max(r - 0.20, 0.0) * 6.5 / extent);
        let corona          = corona_inner * mix(0.40, 1.0, core_blend)
                              * noise_intensity * radial_env * 3.5;

        // ── Layer 3: Ray spikes (moderate) ────────────────────────
        let angle_01   = angle * 0.15915494;
        let ray_fbm_a  = fbm4(vec2<f32>(angle_01 * 18.0 + time_phase * 0.14, r * 4.0 - time_phase * 0.38));
        let ray_fbm_b  = fbm4(vec2<f32>(angle_01 * 28.0 - time_phase * 0.10, r * 3.0 - time_phase * 0.25));
        let ray_fbm    = ray_fbm_a * 0.6 + ray_fbm_b * 0.4;
        let ray_shape  = pow(max(0.0, ray_fbm - 0.33), 2.2) * 3.5;
        let ray_radial = smoothstep(0.225, 0.375, r) * radial_env;
        let rays       = ray_shape * ray_radial;

        let weight = step_size / corona_thickness;
        total_halo   += halo   * weight;
        total_corona += corona * weight;
        total_rays   += rays   * weight;
    }

    // Scale accumulated values
    total_halo   *= 2.0;
    total_corona *= 2.2;
    total_rays   *= 1.6;

    // ── Combine ───────────────────────────────────────────────────
    let combined = total_halo * 0.55 + total_corona * 0.50 + total_rays * 0.35;
    if combined < 0.001 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Smooth colour transition — use smoothstep instead of hard quadratic
    // to avoid visible banding between core_col and halo_col.
    let t_col = smoothstep(0.0, 1.2, combined);
    let col   = mix(color_halo, color_core, t_col);

    let hdr        = total_halo * 5.0 + total_corona * 4.0 + total_rays * 3.5;
    let brightness = combined * hdr;

    let alpha = clamp(pow(combined * 2.2, 1.4), 0.0, 1.0);
    return vec4<f32>(col.rgb * brightness * col.a, alpha);
}
