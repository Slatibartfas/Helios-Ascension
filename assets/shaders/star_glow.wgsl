@group(2) @binding(0) var<uniform> color_core: vec4<f32>;
@group(2) @binding(1) var<uniform> color_halo: vec4<f32>;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Simple pseudo-random hash
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// 2D Noise
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i + vec2<f32>(0.0, 0.0)), hash(i + vec2<f32>(1.0, 0.0)), u.x),
               mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // UV coordinates usually 0.0 to 1.0. Center is 0.5.
    let center = vec2<f32>(0.5, 0.5);
    let uv = in.uv;
    
    // Distance from center
    let dist = distance(uv, center) * 2.0; // 0.0 at center, 1.0 at edge
    
    // Radial Falloff: High intensity at center, rapid drop-off
    // Power function gives a sharp core and long tail
    let intensity = pow(max(0.0, 1.0 - dist), 3.0);
    
    // Noise for texture/corona rays
    // We sample noise based on angle (atan2) and distance
    let angle = atan2(uv.y - 0.5, uv.x - 0.5);
    
    // Add "rays" by varying intensity with angle
    // Frequency 20.0 gives ~20 spikes
    let rays = noise(vec2<f32>(angle * 12.0, 0.0)) * 0.2;
    // Add time-based or position-based noise for detail? For now, static is stable.
    
    // Combine core glow + noise
    var glow = intensity + (intensity * rays);
    
    // Soften the center to avoid a hard point
    glow = clamp(glow, 0.0, 1.0);

    // Color gradient
    // Mix between core (white/hot) and halo (red/orange)
    let final_color = mix(color_halo, color_core, glow);
    
    // Apply exponential falloff to alpha/brightness to ensure edge is transparent
    // smoothstep(edge1, edge0, x) - reverse smoothstep for fade out
    let alpha = smoothstep(1.0, 0.2, dist) * glow;
    
    // Boost brightness for HDR bloom
    // Ensure the core is VERY bright (bloom trigger)
    let brightness = max(alpha * 4.0, 0.0);
    
    return vec4<f32>(final_color.rgb * brightness, alpha);
}
