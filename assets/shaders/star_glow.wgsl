// star_glow.wgsl — Billboard corona & halo shader for stellar bodies
//
// Three-layer model:
//   1. Inverse-square halo   — r⁻² glow starting from the stellar disk edge
//   2. Inner corona ring     — FBM-textured band just outside the disk
//   3. Ray spikes            — high-frequency FBM angular structure with radial fall-off
//
// Billboard size = visual_radius × 8; half-size = visual_radius × 4.
// Star disk occupies UV r ≈ 0.25  (visual_radius / (visual_radius × 4)).

@group(3) @binding(0) var<uniform> color_core: vec4<f32>;
@group(3) @binding(1) var<uniform> color_halo: vec4<f32>;
@group(3) @binding(2) var<uniform> time_phase: f32;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// ── Noise Primitives ──────────────────────────────────────────────────────────

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

// 4-octave FBM for rich, multi-scale corona texture
fn fbm4(p: vec2<f32>) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var pos = p;
    for (var i = 0; i < 4; i++) {
        val += amp * noise2(pos);
        pos   = pos * 2.17 + vec2<f32>(3.1, 7.4);
        amp  *= 0.48;
    }
    return val;   // ~ 0 .. 1
}

// ── Main Fragment ─────────────────────────────────────────────────────────────

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let dx    = in.uv.x - 0.5;
    let dy    = in.uv.y - 0.5;
    let r     = sqrt(dx * dx + dy * dy);
    let angle = atan2(dy, dx);

    // Hard-clip only the true rectangle corners (r > 0.5 is outside the inset circle).
    if r >= 0.5 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    let DISK_UV: f32 = 0.25;

    // ── Master radial envelope ────────────────────────────────────────────────
    // pow(1 - r/0.5, N): guaranteed exactly 0 at r=0.5 with smooth zero-derivative
    // → no geometric boundary can ever be seen regardless of noise on top.
    // Power 2.2 gives a gradual outer falloff that looks like natural limb darkening.
    let radial_env = pow(1.0 - r * 2.0, 2.2);

    // ── Layer 1: Halo ─────────────────────────────────────────────────────────
    let halo_raw   = exp(-r * r * 55.0);
    // Raise floor to 0.40 so the glow stays visible when zoomed close to the star.
    let core_blend = smoothstep(DISK_UV * 0.5, DISK_UV * 1.1, r);
    let halo       = halo_raw * mix(0.40, 1.0, core_blend) * 1.2;

    // ── Layer 2: Corona ───────────────────────────────────────────────────────
    let ct1 = time_phase * 0.25;
    let ct2 = time_phase * 0.38;
    let ct3 = time_phase * 0.16;

    let fbm_a = fbm4(vec2<f32>(angle * 4.8  + ct1,         r * 9.0  - ct1 * 0.8));
    let fbm_b = fbm4(vec2<f32>(angle * 6.7  - ct2 * 0.75,  r * 13.0 - ct2 * 1.1));
    let fbm_c = fbm4(vec2<f32>(angle * 2.9  + ct3 * 0.4,   r * 6.0  - ct3 * 0.5));

    let extent          = 0.55 + 0.45 * fbm_c;
    let noise_intensity = 0.2 + 0.55 * fbm_a + 0.35 * fbm_b;

    // Slower inner decay so bright plumes extend organically before radial_env
    // takes them to zero — the outer shape is formed by noise × radial_env, not
    // by any smoothstep or clip.
    let corona_inner = exp(-max(r - DISK_UV * 0.8, 0.0) * 7.0 / extent);
    let corona       = corona_inner * mix(0.40, 1.0, core_blend) * noise_intensity * radial_env * 3.5;

    // ── Layer 3: Rays ─────────────────────────────────────────────────────────
    let angle_01  = angle * 0.15915494;
    let rt1       = time_phase * 0.08;
    let rt2       = time_phase * 0.05;
    let ray_fbm_a = fbm4(vec2<f32>(angle_01 * 30.0 + rt1,        r * 5.0 - time_phase * 0.28));
    let ray_fbm_b = fbm4(vec2<f32>(angle_01 * 50.0 - rt2 * 0.8,  r * 3.0 - time_phase * 0.17));
    let ray_fbm   = ray_fbm_a * 0.65 + ray_fbm_b * 0.35;
    let ray_shape  = pow(max(0.0, ray_fbm - 0.30), 2.2) * 5.2;
    // Rays also fade via radial_env — no Gaussian clip needed.
    let ray_radial = smoothstep(DISK_UV * 0.9, DISK_UV * 1.5, r) * radial_env;
    let rays = ray_shape * ray_radial;

    // ── Combine ───────────────────────────────────────────────────────────────
    let combined = halo * 0.55 + corona * 0.45 + rays * 0.65;

    let t   = clamp(combined / 1.1, 0.0, 1.0);
    let col = mix(color_halo, color_core, t * t);

    let hdr        = halo * 5.0 + corona * 4.0 + rays * 3.5;
    let brightness = combined * hdr;

    // Alpha: power-curve so dim outer wisps are gently pushed toward zero —
    // no spatial mask, no smoothstep ring, no hard clip drives this.
    let alpha = clamp(pow(combined * 2.2, 1.4), 0.0, 1.0);
    return vec4<f32>(col.rgb * brightness * col.a, alpha);
}
