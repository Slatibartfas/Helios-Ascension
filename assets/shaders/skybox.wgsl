#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings

// Camera uniforms for parallax
@group(2) @binding(0) var<uniform> camera_rotation: mat3x3<f32>;
@group(2) @binding(1) var<uniform> camera_distance: f32;

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
    
    for (var i = 0; i < 4; i++) {
        value += amplitude * noise3d(pp * frequency);
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    
    return value;
}

// Generate star layer
fn generate_stars(direction: vec3<f32>, scale: f32) -> f32 {
    let grid_size = 0.1 * scale;
    let cell = floor(direction / grid_size);
    
    // Check cell and neighbors for stars
    var brightness = 0.0;
    
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            for (var z = -1; z <= 1; z++) {
                let neighbor_cell = cell + vec3<f32>(f32(x), f32(y), f32(z));
                let hash_val = hash13(neighbor_cell);
                
                // Only ~2% of cells have stars
                if (hash_val > 0.98) {
                    // Get star position within cell
                    let star_offset = hash33(neighbor_cell) - 0.5;
                    let star_pos = (neighbor_cell + star_offset) * grid_size;
                    
                    // Calculate angular distance to star
                    let to_star = normalize(star_pos);
                    let angular_dist = acos(clamp(dot(direction, to_star), -1.0, 1.0));
                    
                    // Star size and intensity based on hash
                    let star_size = 0.001 + hash13(neighbor_cell * 1.234) * 0.002;
                    let star_intensity = 0.15 + hash13(neighbor_cell * 5.678) * 0.15;
                    
                    // Create sharp star point
                    let star_brightness = star_intensity * smoothstep(star_size * 2.0, 0.0, angular_dist);
                    brightness += star_brightness;
                }
            }
        }
    }
    
    return brightness;
}

// Generate nebula layer with parallax
fn generate_nebula(direction: vec3<f32>, parallax_factor: f32) -> vec3<f32> {
    // Apply slower parallax to nebula layer
    let parallaxed_dir = direction * parallax_factor;
    
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
    let direction = normalize(in.world_normal);
    
    // Apply camera rotation to direction for parallax
    let rotated_direction = camera_rotation * direction;
    
    // Layer 1: Deep space stars - static, low intensity, tiny
    let star_layer = generate_stars(rotated_direction, 1.0);
    
    // Layer 2: Galactic dust/nebulae with slower parallax (0.5x speed)
    let parallax_factor = 0.5;
    let nebula_layer = generate_nebula(rotated_direction, parallax_factor);
    
    // Combine layers
    var final_color = vec3<f32>(0.0);
    
    // Add star layer (white stars)
    final_color += vec3<f32>(1.0, 0.95, 0.9) * star_layer;
    
    // Add nebula layer
    final_color += nebula_layer;
    
    // Distance fading: dim stars as camera zooms out
    // camera_distance is normalized: 0.0 = min_radius, 1.0 = max_radius
    let distance_fade = 1.0 - (camera_distance * 0.3); // Reduce by up to 30%
    final_color *= distance_fade;
    
    // Luminance cap: no pixel should exceed 0.4 brightness
    let luminance = dot(final_color, vec3<f32>(0.299, 0.587, 0.114));
    if (luminance > 0.4) {
        final_color *= 0.4 / luminance;
    }
    
    return vec4<f32>(final_color, 1.0);
}
