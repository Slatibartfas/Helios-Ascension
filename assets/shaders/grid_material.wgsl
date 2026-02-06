#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings
#import bevy_pbr::mesh_functions

@group(2) @binding(0) var<uniform> grid_params: GridParams;

struct GridParams {
    grid_scale: f32,      // Scale of the grid cells
    fade_distance: f32,   // Distance at which grid starts to fade
    max_distance: f32,    // Distance at which grid is fully transparent
    alpha: f32,           // Base alpha value (0.1 for subtle effect)
}

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Constants
const TAU: f32 = 6.283185307; // 2π for circular calculations

// Simple circular grid pattern function
fn circular_grid(pos: vec2<f32>, scale: f32) -> f32 {
    let dist = length(pos);
    
    // Radial lines
    let radial = abs(fract(dist / scale) - 0.5) * 2.0;
    let radial_line = smoothstep(0.95, 0.97, radial);
    
    // Angular lines (16 divisions around the circle)
    let angle = atan2(pos.y, pos.x);
    let angular = abs(fract(angle * 8.0 / TAU) - 0.5) * 2.0;
    let angular_line = smoothstep(0.95, 0.97, angular);
    
    return max(radial_line, angular_line);
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    // Use world position XY for grid coordinates (Z is up/down from ecliptic)
    let grid_pos = in.world_position.xy;
    
    // Calculate distance from center (sun at origin)
    let dist_from_center = length(grid_pos);
    
    // Use circular grid pattern for space-themed look
    let grid_value = circular_grid(grid_pos, grid_params.grid_scale);
    
    // Calculate fade based on distance from center
    let fade = 1.0 - smoothstep(
        grid_params.fade_distance,
        grid_params.max_distance,
        dist_from_center
    );
    
    // Grid color - subtle cyan/blue for sci-fi look
    let grid_color = vec3<f32>(0.2, 0.6, 0.8);
    
    // Final alpha combines grid lines, fade, and base alpha
    let alpha = grid_value * fade * grid_params.alpha;
    
    return vec4<f32>(grid_color, alpha);
}
