#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_view_bindings::globals

@group(2) @binding(0) var<uniform> camera_rotation: mat3x3<f32>;
@group(2) @binding(1) var<uniform> camera_distance: f32;

// --- Cinematic Constants ---
const STAR_DENSITY: f32 = 120.0;     // Higher = smaller, more numerous stars
const STAR_BRIGHTNESS: f32 = 5.0;    // Over 1.0 triggers HDR Bloom
const NEBULA_STRENGTH: f32 = 0.15;
const MILKY_WAY_STRENGTH: f32 = 0.4;

// --- Core Math ---
fn hash33(p: vec3<f32>) -> vec3<f32> {
    var p3 = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yxz + 33.33);
    return fract((p3.xxy + p3.yxx) * p3.zyx);
}

fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(dot(hash33(i + vec3<f32>(0.0, 0.0, 0.0)), f - vec3<f32>(0.0, 0.0, 0.0)),
                       dot(hash33(i + vec3<f32>(1.0, 0.0, 0.0)), f - vec3<f32>(1.0, 0.0, 0.0)), u.x),
                   mix(dot(hash33(i + vec3<f32>(0.0, 1.0, 0.0)), f - vec3<f32>(0.0, 1.0, 0.0)),
                       dot(hash33(i + vec3<f32>(1.0, 1.0, 0.0)), f - vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y),
               mix(mix(dot(hash33(i + vec3<f32>(0.0, 0.0, 1.0)), f - vec3<f32>(0.0, 0.0, 1.0)),
                       dot(hash33(i + vec3<f32>(1.0, 0.0, 1.0)), f - vec3<f32>(1.0, 0.0, 1.0)), u.x),
                   mix(dot(hash33(i + vec3<f32>(0.0, 1.0, 1.0)), f - vec3<f32>(0.0, 1.0, 1.0)),
                       dot(hash33(i + vec3<f32>(1.0, 1.0, 1.0)), f - vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y), u.z) + 0.5;
}

fn fbm(p: vec3<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pos = p;
    for (var i = 0; i < 5; i++) {
        v += a * noise(pos);
        pos *= 2.0;
        a *= 0.5;
    }
    return v;
}

// --- Layer Generators ---

// Pinpoint stars using a sparse jittered grid
fn get_stars(dir: vec3<f32>) -> vec3<f32> {
    let p = dir * STAR_DENSITY;
    let i = floor(p);
    let f = fract(p);
    let h = hash33(i);
    
    // Twinkle effect using Bevy's globals.time
    let twinkle = sin(globals.time * (h.x * 5.0) + h.y * 10.0) * 0.5 + 0.5;
    
    // Distance to jittered point in cell
    let dist = distance(f, h);
    let intensity = smoothstep(0.04, 0.0, dist) * h.z * twinkle;
    
    // Star color temperature (Blue-ish to Red-ish)
    let col = mix(vec3<f32>(0.7, 0.8, 1.0), vec3<f32>(1.0, 0.5, 0.3), h.y);
    return intensity * col * STAR_BRIGHTNESS;
}

// Organic nebula using Domain Warping
fn get_nebula(dir: vec3<f32>) -> vec3<f32> {
    let p = dir * 1.5;
    // Layer 1: Distortion coordinates
    let q = vec3<f32>(fbm(p), fbm(p + vec3<f32>(1.2)), fbm(p + vec3<f32>(2.8)));
    // Layer 2: Final warped noise
    let n = fbm(p + 2.0 * q);
    
    let base_col = vec3<f32>(0.05, 0.02, 0.1); // Deep Purple
    let highlight = vec3<f32>(0.1, 0.2, 0.3); // Cosmic Blue
    
    return mix(base_col, highlight, n) * n * NEBULA_STRENGTH;
}

// Milky Way with dust lanes (Ridged Noise)
fn get_milky_way(dir: vec3<f32>) -> vec3<f32> {
    // Tilt the galaxy
    let tilted_dir = camera_rotation * dir;
    let band = exp(-pow(tilted_dir.y * 6.0, 2.0)); // The "streak" shape
    
    // Ridged noise creates the "voids" and dust lanes
    let n = 1.0 - abs(fbm(tilted_dir * 3.0 + 0.5));
    let dust = smoothstep(0.2, 0.7, n);
    
    let core_col = vec3<f32>(0.5, 0.3, 0.2); // Golden core
    return core_col * band * dust * MILKY_WAY_STRENGTH;
}

struct FragmentInput {
    @location(1) world_normal: vec3<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_normal);
    
    // Combine layers with additive blending
    var final_color = vec3<f32>(0.0);
    
    final_color += get_nebula(dir);
    final_color += get_milky_way(dir);
    final_color += get_stars(dir);
    
    // Distance fade (keep a tiny bit of brightness for deep space)
    let fade = clamp(1.0 - (camera_distance * 0.5), 0.05, 1.0);
    
    // Apply a simple Exposure tone mapping hint
    // (Actual tonemapping happens in Bevy's post-processing)
    return vec4(final_color * fade, 1.0);
}
