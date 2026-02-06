#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings

// Camera uniforms for parallax
@group(2) @binding(0) var<uniform> camera_rotation: mat3x3<f32>;
@group(2) @binding(1) var<uniform> camera_distance: f32;

// Visual parameters - extracted as constants for easy tuning
const FBM_OCTAVES: i32 = 4;                  // Number of noise octaves for nebulae
const STAR_GRID_SIZE: f32 = 0.01;            // Grid cell size for star placement (smaller = more stars)
const STAR_DENSITY_THRESHOLD: f32 = 0.95;    // ~5% of cells contain stars (more visible)
const STAR_SIZE_MIN: f32 = 0.0001;           // Minimum star angular size (much smaller)
const STAR_SIZE_RANGE: f32 = 0.0002;         // Star size variation range
const STAR_INTENSITY_MIN: f32 = 0.2;         // Minimum star brightness (increased)
const STAR_INTENSITY_RANGE: f32 = 0.25;      // Star intensity variation range (increased)
const NEBULA_PARALLAX_FACTOR: f32 = 0.5;     // Nebula layer moves at 0.5x speed for depth
const LUMINANCE_CAP: f32 = 0.4;              // Maximum brightness for any pixel
const DISTANCE_FADE_AMOUNT: f32 = 0.3;       // Maximum dimming at far distance (30%)

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Hash function for procedural star placement
// Based on https://www.shadertoy.com/view/4djSRW
fn hash13(p3: vec3<f32>) -> f32 {
    var p = fract(p3 * 0.1031);
    p += dot(p, p.zyx + 31.32);
    return fract((p.x + p.y) * p.z);
}

fn hash33(p3: vec3<f32>) -> vec3<f32> {
    var p = fract(p3 * vec3<f32>(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    return fract((p.xxy + p.yxx) * p.zyx);
}

// Simplex-like noise function for nebulae
fn noise3d(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    
    // Smooth interpolation
    let u = f * f * (3.0 - 2.0 * f);
    
    // Sample 8 corners of the cube
    let c000 = hash13(i + vec3<f32>(0.0, 0.0, 0.0));
    let c100 = hash13(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash13(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash13(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash13(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash13(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash13(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash13(i + vec3<f32>(1.0, 1.0, 1.0));
    
    // Trilinear interpolation
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    
    let y0 = mix(x00, x10, u.y);
    let y1 = mix(x01, x11, u.y);
    
    return mix(y0, y1, u.z);
}

// Fractal Brownian Motion for layered noise
fn fbm(p: vec3<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var frequency = 1.0;
    var pp = p;
    
    for (var i = 0; i < FBM_OCTAVES; i++) {
        value += amplitude * noise3d(pp * frequency);
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    
    return value;
}

// Generate star layer
fn generate_stars(direction: vec3<f32>, scale: f32) -> f32 {
    let grid_size = STAR_GRID_SIZE * scale;
    let cell = floor(direction / grid_size);
    
    // Check cell and neighbors for stars
    var brightness = 0.0;
    
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            for (var z = -1; z <= 1; z++) {
                let neighbor_cell = cell + vec3<f32>(f32(x), f32(y), f32(z));
                let hash_val = hash13(neighbor_cell);
                
                // Only ~5% of cells have stars (based on STAR_DENSITY_THRESHOLD)
                if (hash_val > STAR_DENSITY_THRESHOLD) {
                    // Get star position within cell
                    let star_offset = hash33(neighbor_cell) - 0.5;
                    let star_pos = (neighbor_cell + star_offset) * grid_size;
                    
                    // Calculate direction to star
                    let to_star = normalize(star_pos);
                    let dot_val = clamp(dot(direction, to_star), -1.0, 1.0);
                    
                    // Star size and intensity based on hash
                    let star_size = STAR_SIZE_MIN + hash13(neighbor_cell * 1.234) * STAR_SIZE_RANGE;
                    let star_intensity = STAR_INTENSITY_MIN + hash13(neighbor_cell * 5.678) * STAR_INTENSITY_RANGE;
                    
                    // Create sharp star point using dot-product falloff (avoids expensive acos)
                    let cos_max_angle = cos(star_size * 2.0);
                    let star_brightness = star_intensity * smoothstep(cos_max_angle, 1.0, dot_val);
                    brightness += star_brightness;
                }
            }
        }
    }
    
    return brightness;
}

// Generate nebula layer with parallax
fn generate_nebula(direction: vec3<f32>, parallax_factor: f32) -> vec3<f32> {
    // Apply slower rotation for parallax effect by reducing the camera rotation influence
    // This makes nebulae appear to move slower than stars when camera rotates
    let parallaxed_dir = mix(direction, normalize(vec3<f32>(direction.x, direction.y, direction.z * 0.8)), 1.0 - parallax_factor);
    
    // Sample noise at multiple scales
    let noise_scale = 2.0;
    let noise_val = fbm(parallaxed_dir * noise_scale);
    
    // Create nebula color based on noise
    // Use desaturated dark blue/purple as specified
    let base_color = vec3<f32>(0.08, 0.05, 0.15); // Dark blue-purple
    let accent_color = vec3<f32>(0.12, 0.08, 0.20); // Slightly lighter purple
    
    let nebula_color = mix(base_color, accent_color, noise_val);
    
    // Very faint nebulae - keep intensity low
    let intensity = noise_val * 0.04; // Very faint
    
    return nebula_color * intensity;
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Get direction from sphere center to fragment (normalized view direction)
    // This already accounts for camera orientation via the view matrix
    let direction = normalize(in.world_normal);
    
    // Layer 1: Deep space stars - static, low intensity, tiny
    let star_layer = generate_stars(direction, 1.0);
    
    // Layer 2: Galactic dust/nebulae with slower parallax
    let nebula_layer = generate_nebula(direction, NEBULA_PARALLAX_FACTOR);
    
    // Combine layers
    var final_color = vec3<f32>(0.0);
    
    // Add star layer (white stars)
    final_color += vec3<f32>(1.0, 0.95, 0.9) * star_layer;
    
    // Add nebula layer
    final_color += nebula_layer;
    
    // Distance fading: dim stars as camera zooms out
    // camera_distance is normalized: 0.0 = min_radius, 1.0 = max_radius
    let distance_fade = 1.0 - (camera_distance * DISTANCE_FADE_AMOUNT);
    final_color *= distance_fade;
    
    // Luminance cap: no pixel should exceed LUMINANCE_CAP brightness
    let luminance = dot(final_color, vec3<f32>(0.299, 0.587, 0.114));
    if (luminance > LUMINANCE_CAP) {
        final_color *= LUMINANCE_CAP / luminance;
    }
    
    return vec4<f32>(final_color, 1.0);
}
