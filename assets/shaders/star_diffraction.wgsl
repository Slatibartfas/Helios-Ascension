// star_diffraction.wgsl — Diffraction spike / lens-flare billboard shader
//
// Simulates the cross-shaped diffraction spikes produced by a camera's aperture
// blades when photographing a bright point source (the classic "star of Bethlehem"
// look in astrophotography).
//
// Spike pattern:
//   Primary (+) cross  : pow(|cos(2·θ)|, N)   → 4 spikes at  0° / 90° / 180° / 270°
//   Secondary (×) cross: pow(|cos(2·θ + π/4)|, N) × 0.55   → 4 spikes at 45° intervals
//
// Applied to a larger billboard (visual_radius × 30) so the spikes extend well
// beyond the corona.  LOD is handled by the Rust side.

@group(3) @binding(0) var<uniform> color: vec4<f32>;

struct FragmentInput {
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let dx = in.uv.x - 0.5;
    let dy = in.uv.y - 0.5;
    let r  = sqrt(dx * dx + dy * dy);

    // Avoid division-by-zero at the very centre
    if r < 0.002 {
        return vec4<f32>(color.rgb * 12.0 * color.a, 1.0);
    }

    let angle = atan2(dy, dx);   // −π .. π

    // ── Diffraction spike pattern ─────────────────────────────────────────────
    // cos(2·angle)² oscillates 4× per full rotation → 4 lobes (+ cross)
    // High power sharpens them into narrow spikes
    let SPIKE_POWER: f32 = 26.0;

    let cos2a    = cos(2.0 * angle);
    let cos2a_45 = cos(2.0 * angle + 0.78539816);   // rotated 45° (π/4)

    let spikes_primary   = pow(abs(cos2a),    SPIKE_POWER);
    let spikes_secondary = pow(abs(cos2a_45), SPIKE_POWER) * 0.55;

    let spike_pattern = spikes_primary + spikes_secondary;

    // ── Radial intensity: inverse-power from centre, hard cut at edge ─────────
    // Spike brightness falls as r^-1.6 from the star centre.
    // Clamp keeps it finite at small r.
    let r_safe       = max(r, 0.008);
    let spike_radial = min(0.0006 / pow(r_safe, 1.6), 1.0)
                     * smoothstep(0.5, 0.05, r);    // fade out near billboard edge

    // ── Combine ───────────────────────────────────────────────────────────────
    let total = spike_pattern * spike_radial;

    // HDR boost so spikes trigger bloom and appear as bright streaks
    let hdr_col  = color.rgb * total * 7.0 * color.a;
    let alpha    = clamp(total, 0.0, 1.0);

    return vec4<f32>(hdr_col, alpha);
}
