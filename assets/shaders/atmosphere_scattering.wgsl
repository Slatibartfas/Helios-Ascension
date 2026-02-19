// Atmospheric scattering shader — single-scattering Rayleigh + Henyey-Greenstein Mie.
//
// Renders on a translucent sphere slightly larger than the planet surface.
// The fragment shader ray-marches from the camera through the atmosphere shell,
// accumulating in-scattered light from the primary light source (Sun).
//
// planet_center is derived from fragment geometry (world_pos - normal * atmo_r)
// so that the effect stays correct as planets move along their orbits without
// needing a per-frame uniform update.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_bindings
#import bevy_pbr::mesh_functions

// Material uniforms — bound by AsBindGroup at @group(3).
// beta_rayleigh.xyz = Rayleigh colour tint, .w = strength multiplier
@group(3) @binding(0) var<uniform> beta_rayleigh: vec4<f32>;
// beta_mie.xyz = haze colour, .w = Mie asymmetry parameter g
@group(3) @binding(1) var<uniform> beta_mie: vec4<f32>;
// atmo_params.x = planet surface radius, .y = atmosphere outer radius,
//            .z = scale height (visual units), .w = Mie intensity
@group(3) @binding(2) var<uniform> atmo_params: vec4<f32>;
// sun_dir.xyz = unused (computed from geometry), .w = quality (0/1/2)
@group(3) @binding(3) var<uniform> sun_dir: vec4<f32>;
// planet_center.xyz = unused (computed from geometry), kept for bind-group compat
@group(3) @binding(4) var<uniform> planet_center: vec4<f32>;

const PI: f32 = 3.14159265359;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// ── Ray-sphere intersection ────────────────────────────────────────
// Returns (t_near, t_far). If no intersection, t_near > t_far.
fn ray_sphere(origin: vec3<f32>, dir: vec3<f32>, center: vec3<f32>, radius: f32) -> vec2<f32> {
    let oc = origin - center;
    let b = dot(oc, dir);
    let c = dot(oc, oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return vec2<f32>(1e20, -1e20); // no hit
    }
    let sq = sqrt(disc);
    return vec2<f32>(-b - sq, -b + sq);
}

// ── Density at a given altitude ────────────────────────────────────
fn atmosphere_density(sample_pos: vec3<f32>, center: vec3<f32>, planet_r: f32, scale_h: f32) -> f32 {
    let altitude = length(sample_pos - center) - planet_r;
    return exp(-max(altitude, 0.0) / scale_h);
}

// ── Henyey-Greenstein phase function ───────────────────────────────
fn hg_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (4.0 * PI * pow(denom, 1.5));
}

// ── Rayleigh phase function ────────────────────────────────────────
fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta);
}

// ── Optical depth along a ray segment (numerical integration) ─────
fn optical_depth(origin: vec3<f32>, dir: vec3<f32>, length_: f32, center: vec3<f32>, planet_r: f32, scale_h: f32, steps: i32) -> f32 {
    let step_size = length_ / f32(steps);
    var depth = 0.0;
    for (var i = 0; i < steps; i = i + 1) {
        let t = (f32(i) + 0.5) * step_size;
        let sample_pos = origin + dir * t;
        depth += atmosphere_density(sample_pos, center, planet_r, scale_h) * step_size;
    }
    return depth;
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let planet_r = atmo_params.x;
    let atmo_r = atmo_params.y;
    let scale_h = atmo_params.z;
    let mie_intensity = atmo_params.w;
    let quality = sun_dir.w;

    // ── Derive planet centre from fragment geometry ────────────────
    // For a sphere mesh centred on the planet, world_normal points radially
    // outward.  point − normal * radius = centre.  This is always correct
    // regardless of where the planet has moved along its orbit.
    let center = in.world_position.xyz - normalize(in.world_normal) * atmo_r;

    // ── Sun direction — Sun is at world origin ────────────────────
    let light_dir = normalize(-center); // from planet toward origin

    // Camera position from the view matrix
    let cam_pos = view.world_position.xyz;
    let ray_dir = normalize(in.world_position.xyz - cam_pos);

    // ── Intersect the view ray with the atmosphere sphere ──────────
    let atmo_hit = ray_sphere(cam_pos, ray_dir, center, atmo_r);
    if atmo_hit.x > atmo_hit.y {
        // Miss — fully transparent
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Clip near hit to 0 (camera may be inside the atmosphere)
    let t_near = max(atmo_hit.x, 0.0);
    // Clip far hit to planet surface if the view ray enters the planet
    let planet_hit = ray_sphere(cam_pos, ray_dir, center, planet_r);
    var t_far = atmo_hit.y;
    if planet_hit.x < planet_hit.y && planet_hit.x > 0.0 {
        t_far = min(t_far, planet_hit.x);
    }

    let ray_length = t_far - t_near;
    if ray_length <= 0.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // ── Choose sample count based on quality ───────────────────────
    var num_samples: i32;
    var num_light_samples: i32;
    if quality < 0.5 {
        // Low — very cheap rim glow only (2 + 1)
        num_samples = 4;
        num_light_samples = 2;
    } else if quality < 1.5 {
        // Medium (default)
        num_samples = 8;
        num_light_samples = 4;
    } else {
        // High
        num_samples = 16;
        num_light_samples = 8;
    }

    let step_size = ray_length / f32(num_samples);

    // Scattering coefficients — scaled for game-visual visibility.
    // The strength multiplier already encodes pressure/composition differences,
    // so we apply a moderate base factor to make even thin atmospheres show
    // a visible limb glow.
    let beta_r = beta_rayleigh.xyz * beta_rayleigh.w * 0.8;
    let beta_m_val = mie_intensity * 3.0;
    let mie_color = beta_mie.xyz;
    let g = beta_mie.w;

    // Phase function values (constant along the ray for a single light source)
    let cos_theta = dot(ray_dir, light_dir);
    let phase_r = rayleigh_phase(cos_theta);
    let phase_m = hg_phase(cos_theta, g);

    // ── Ray-march accumulation ─────────────────────────────────────
    var total_rayleigh = vec3<f32>(0.0);
    var total_mie = vec3<f32>(0.0);
    var optical_depth_r = 0.0;
    var optical_depth_m = 0.0;

    for (var i = 0; i < num_samples; i = i + 1) {
        let t = t_near + (f32(i) + 0.5) * step_size;
        let sample_pos = cam_pos + ray_dir * t;
        let density = atmosphere_density(sample_pos, center, planet_r, scale_h);
        let d_r = density * step_size;
        let d_m = density * step_size;

        optical_depth_r += d_r;
        optical_depth_m += d_m;

        // ── Light ray: optical depth from sample to atmosphere edge toward Sun ──
        let sun_hit = ray_sphere(sample_pos, light_dir, center, atmo_r);
        let sun_ray_len = max(sun_hit.y, 0.0);
        let sun_od = optical_depth(sample_pos, light_dir, sun_ray_len, center, planet_r, scale_h, num_light_samples);

        // Check if light ray intersects the planet (shadow)
        let sun_planet = ray_sphere(sample_pos, light_dir, center, planet_r);
        let in_shadow = sun_planet.x < sun_planet.y && sun_planet.x > 0.0;

        if !in_shadow {
            let total_od = optical_depth_r + sun_od;
            let attenuation = exp(-(beta_r * total_od + vec3<f32>(beta_m_val * (optical_depth_m + sun_od))));
            total_rayleigh += d_r * attenuation;
            total_mie += d_m * attenuation;
        }
    }

    // ── Combine Rayleigh and Mie scattering ────────────────────────
    let color = (total_rayleigh * beta_r * phase_r)
              + (total_mie * mie_color * beta_m_val * phase_m);

    // Clamp and compute alpha from total brightness.
    // Use a generous multiplier so thin atmospheres (Mars) are still visible.
    let brightness = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    let alpha = clamp(brightness * 4.0, 0.0, 0.92);

    return vec4<f32>(color, alpha);
}
