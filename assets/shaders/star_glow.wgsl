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
    let r     = sqrt(dx * dx + dy * dy);   // 0 at centre, ~0.5 at billboard edge
    let angle = atan2(dy, dx);             // −π .. π

    // Clip billboard corners → circular effect
    if r > 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Star disk UV radius (disk_r / billboard_half_size = 1/4 = 0.25)
    let DISK_UV:  f32 = 0.25;
    // Reference distance from disk edge where halo normalises to 1.0
    // Scaled proportionally with DISK_UV (ratio ~0.56 kept from the original).
    let REF_DIST: f32 = 0.14;

    // ── Layer 1: Inverse-square halo (I ∝ r⁻²) ──────────────────────────────
    let r_from_disk = max(r - DISK_UV + REF_DIST, REF_DIST * 0.1);
    let halo_raw    = (REF_DIST * REF_DIST) / (r_from_disk * r_from_disk);
    let halo        = clamp(halo_raw, 0.0, 1.0) * smoothstep(0.5, 0.18, r);

    // ── Layer 2: Inner corona ring with FBM structure ─────────────────────────
    // Narrow band immediately outside the stellar disk
    let corona_band = smoothstep(DISK_UV * 0.85, DISK_UV * 1.1, r)
                    * smoothstep(DISK_UV * 1.8,  DISK_UV * 1.1, r);
    // FBM on (angle × freq, r) → ~28 angular features per 2π
    let fbm_in  = fbm4(vec2<f32>(angle * 4.5, r * 6.0));
    let corona  = corona_band * (0.2 + 0.8 * fbm_in) * 2.0;

    // ── Layer 3: Ray spikes — FBM angular noise, radial extent ───────────────
    // High angular frequency gives many thin, uneven rays
    let angle_01  = angle * 0.15915494;              // 0 .. 1 wrap
    let ray_fbm   = fbm4(vec2<f32>(angle_01 * 34.0, 0.9));  // ~34 angular periods
    // Threshold + power-sharpen: only peaks form visible rays
    let ray_shape  = pow(max(0.0, ray_fbm - 0.32), 2.5) * 4.5;
    // Rays extend from disk edge outward, fading beyond 60 % of billboard radius
    let ray_radial = smoothstep(DISK_UV * 0.9, DISK_UV * 1.4, r)
                   * smoothstep(0.5, DISK_UV * 1.6, r);
    let rays = ray_shape * ray_radial;

    // ── Combine ───────────────────────────────────────────────────────────────
    let combined = halo * 0.55 + corona * 0.45 + rays * 0.65;

    // Color: bright parts are hot-white, dim parts are golden-orange
    let t   = clamp(combined / 1.1, 0.0, 1.0);
    let col = mix(color_halo, color_core, t * t);

    // HDR multipliers trigger bloom on bright regions
    let hdr        = halo * 6.0 + corona * 4.5 + rays * 3.5 + 0.4;
    let brightness = combined * hdr;

    let alpha = clamp(combined, 0.0, 1.0) * smoothstep(0.5, 0.44, r);
    return vec4<f32>(col.rgb * brightness * col.a, alpha);
}
