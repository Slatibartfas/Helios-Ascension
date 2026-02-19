use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// Plugin that registers the atmosphere scattering material and systems.
pub struct AtmospherePlugin;

impl Plugin for AtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<AtmosphereMaterial>::default())
            .init_resource::<AtmosphereSettings>();
    }
}

/// Global settings that control atmospheric scattering rendering.
#[derive(Resource, Debug, Clone)]
pub struct AtmosphereSettings {
    /// Master toggle for atmospheric scattering.
    pub enabled: bool,
    /// Quality preset — controls ray-march sample count in the shader.
    /// 0 = Low (rim glow only), 1 = Medium (default), 2 = High.
    pub quality: u32,
    /// Global intensity multiplier applied on top of per-body values.
    pub global_intensity: f32,
}

impl Default for AtmosphereSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            quality: 1,
            global_intensity: 1.0,
        }
    }
}

/// Marker component for the atmosphere shell child entity.
/// Stores the parent body entity so systems can look up body-specific data.
#[derive(Component, Debug, Clone, Copy)]
pub struct AtmosphereShell {
    pub body_entity: Entity,
}

/// Custom material for single-scattering atmospheric rendering.
///
/// Applied to a translucent sphere slightly larger than the planet surface.
/// The fragment shader integrates optical depth along the view ray through an
/// exponential-density atmosphere, computing Rayleigh + Henyey-Greenstein Mie
/// scattering from the primary light source (Sun).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct AtmosphereMaterial {
    /// Rayleigh scattering coefficients (RGB). Controls sky colour.
    #[uniform(0)]
    pub beta_rayleigh: Vec4, // .xyz = coefficients, .w = strength

    /// Mie scattering parameters. .xyz = haze colour, .w = mie_g (asymmetry).
    #[uniform(1)]
    pub beta_mie: Vec4,

    /// Atmosphere geometry.
    /// .x = planet surface radius (visual units)
    /// .y = atmosphere outer radius (visual units)
    /// .z = scale height (visual units, controls density falloff)
    /// .w = intensity multiplier
    #[uniform(2)]
    pub atmo_params: Vec4,

    /// Sun direction (normalised, world-space). .w = quality (0/1/2).
    #[uniform(3)]
    pub sun_dir: Vec4,

    /// Planet centre position (world-space). .w unused.
    #[uniform(4)]
    pub planet_center: Vec4,
}

impl Material for AtmosphereMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/atmosphere_scattering.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        // Render just behind the cloud layer but in front of far-field objects
        0.0
    }
}

impl AtmosphereMaterial {
    /// Construct a material from derived scattering parameters.
    ///
    /// - `planet_radius`: visual radius of the surface mesh (game units, NOT km).
    /// - `atmo`: the `AtmosphereComposition` component with derived scattering fields.
    /// - `planet_pos`: current world-space position of the planet entity.
    /// - `sun_pos`: world-space position of the sun (usually origin).
    /// - `quality`: quality preset (0/1/2).
    pub fn from_composition(
        planet_radius: f32,
        atmo: &crate::astronomy::components::AtmosphereComposition,
        planet_pos: Vec3,
        sun_pos: Vec3,
        quality: u32,
    ) -> Self {
        // Atmosphere shell extends ~5% above surface for visual effect.
        let atmo_radius = planet_radius * 1.05;
        let shell_thickness = atmo_radius - planet_radius;

        // Map physical scale height to a visible fraction of the shell.
        // Dividing by 50 means:  Earth 8.5 km → 0.17,  Mars 10.9 → 0.22,
        // Titan ~21 → 0.42,  Venus ~16 → 0.32.   Clamped so even the
        // thinnest atmospheres get a minimum 20% of the shell with
        // significant density, and thick ones cap at 80%.
        let scale_fraction = (atmo.scale_height_km / 50.0).clamp(0.20, 0.80);
        let visual_scale_height = shell_thickness * scale_fraction;

        let sun_dir = (sun_pos - planet_pos).normalize_or_zero();

        Self {
            beta_rayleigh: Vec4::new(
                atmo.rayleigh_rgb[0],
                atmo.rayleigh_rgb[1],
                atmo.rayleigh_rgb[2],
                atmo.rayleigh_strength,
            ),
            beta_mie: Vec4::new(
                atmo.haze_color[0],
                atmo.haze_color[1],
                atmo.haze_color[2],
                atmo.mie_g,
            ),
            atmo_params: Vec4::new(
                planet_radius,
                atmo_radius,
                visual_scale_height,
                atmo.atmosphere_intensity * atmo.mie_strength,
            ),
            sun_dir: Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, quality as f32),
            planet_center: Vec4::new(planet_pos.x, planet_pos.y, planet_pos.z, 0.0),
        }
    }
}
