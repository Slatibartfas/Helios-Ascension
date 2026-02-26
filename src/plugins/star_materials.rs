use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

use crate::astronomy::components::{CurrentStarSystem, SystemId};
use super::solar_system::CelestialBody;

/// Material for the star glow/corona effect (billboard)
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarGlowMaterial {
    #[uniform(0)]
    pub color_core: Vec4,
    #[uniform(1)]
    pub color_halo: Vec4,
    /// Elapsed time (seconds) driving corona/ray animation.
    /// Updated each frame by `update_glow_time` system.
    #[uniform(2)]
    pub time_phase: f32,
}

impl Material for StarGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/star_glow.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn depth_bias(&self) -> f32 {
        // Negative keeps this billboard in front of the star sphere so the
        // additive glow actually renders ON the disk, not behind it.
        -1.0
    }
}

/// Limb-darkening material applied directly to the star sphere mesh.
/// Replaces StandardMaterial for star bodies so the disk darkens at the edges
/// (Eddington limb-darkening law) and shows a cool-to-hot temperature gradient.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarSurfaceMaterial {
    /// Centre colour (hot white core, HDR > 1.0)
    #[uniform(0)]
    pub color_center: Vec4,
    /// Limb colour (cooler orange-red at disk edge)
    #[uniform(1)]
    pub color_limb: Vec4,
    /// Optional surface texture (solar granulation etc.)
    #[texture(2)]
    #[sampler(3)]
    pub star_texture: Option<Handle<Image>>,
}

impl Material for StarSurfaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/star_surface.wgsl".into()
    }
    // Stars are self-illuminating
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}

/// Large billboard for diffraction spikes / lens flare, rendered behind the corona.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarDiffractionMaterial {
    /// Spike colour (usually warm white, HDR > 1.0)
    #[uniform(0)]
    pub color: Vec4,
}

impl Material for StarDiffractionMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/star_diffraction.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn depth_bias(&self) -> f32 {
        // Slightly more negative than corona (-1.0) so diffraction renders
        // behind the corona but still in front of the star sphere.
        -2.0
    }
}

/// Component to make an entity always face the camera (e.g. sun glare)
#[derive(Component)]
pub struct Billboard;

/// Component to tag the star glare entity for LOD updates
#[derive(Component)]
pub struct StarGlare {
    pub base_core_color: Vec4,
    pub base_halo_color: Vec4,
    /// Visual radius of the parent star in game units.
    /// Used to scale LOD fade distances so all star sizes behave consistently.
    pub visual_radius: f32,
}

/// Component to tag the diffraction spike billboard for LOD updates
#[derive(Component)]
pub struct StarDiffraction {
    pub base_color: Vec4,
    /// Visual radius of the parent star in game units.
    pub visual_radius: f32,
}

// ── 3D Volumetric Corona Materials ──────────────────────────────────────────

/// Inner volumetric corona shell — ray-marched 3D FBM plasma.
/// Applied to a sphere at ~1.75× star radius with `AlphaMode::Add`.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarCorona3dMaterial {
    #[uniform(0)]
    pub color_core: Vec4,
    #[uniform(1)]
    pub color_halo: Vec4,
    #[uniform(2)]
    pub time_phase: f32,
    /// corona_params.x = star surface radius, .y = corona shell outer radius
    #[uniform(3)]
    pub corona_params: Vec4,
}

impl Material for StarCorona3dMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/star_corona_3d.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn depth_bias(&self) -> f32 {
        1.0
    }
}

/// Outer diffuse halo shell — limb-brightening + coarse streamer noise.
/// Applied to a sphere at ~4× star radius with `AlphaMode::Add`.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarHalo3dMaterial {
    #[uniform(0)]
    pub color_halo: Vec4,
    #[uniform(1)]
    pub time_phase: f32,
    /// halo_params.x = star surface radius, .y = halo shell outer radius
    #[uniform(2)]
    pub halo_params: Vec4,
}

impl Material for StarHalo3dMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/star_halo_3d.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn depth_bias(&self) -> f32 {
        2.0
    }
}

/// Marker component for the inner 3D volumetric corona shell.
#[derive(Component)]
pub struct StarCoronaShell {
    pub base_core_color: Vec4,
    pub base_halo_color: Vec4,
    pub visual_radius: f32,
}

/// Marker component for the outer 3D halo shell.
#[derive(Component)]
pub struct StarHaloShell {
    pub base_halo_color: Vec4,
    pub visual_radius: f32,
}

pub(super) fn update_billboards(
    mut query: Query<(&mut Transform, &GlobalTransform, &ChildOf), With<Billboard>>,
    parent_query: Query<&GlobalTransform, Without<Billboard>>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
) {
    if let Ok(camera_global_transform) = camera_query.single() {
        let camera_pos = camera_global_transform.translation();
        for (mut transform, _global, parent) in query.iter_mut() {
            // Compute the billboard's world position from its parent
            let parent_global = parent_query
                .get(parent.parent())
                .map(|g| g.compute_transform())
                .unwrap_or_default();

            let world_pos = parent_global.transform_point(transform.translation);

            // Compute world-space rotation that faces the camera
            let forward = (camera_pos - world_pos).normalize_or_zero();
            if forward.length_squared() < 0.001 {
                continue;
            }
            let world_rotation = Transform::IDENTITY.looking_at(-forward, Vec3::Y).rotation;

            // Convert to local space by removing the parent's rotation
            transform.rotation = parent_global.rotation.inverse() * world_rotation;
        }
    }
}

/// Updates visibility of celestial bodies based on the current star system
pub(super) fn update_body_visibility(
    current_system: Res<CurrentStarSystem>,
    mut param_set: ParamSet<(
        // Case 1: System Changed - update everyone
        Query<(&mut Visibility, &SystemId), With<CelestialBody>>,
        // Case 2: System Stable - update only new/changed bodies
        Query<(&mut Visibility, &SystemId), (With<CelestialBody>, Changed<SystemId>)>,
    )>,
) {
    if current_system.is_changed() {
        for (mut vis, system_id) in param_set.p0().iter_mut() {
            *vis = if system_id.0 == current_system.0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    } else {
        for (mut vis, system_id) in param_set.p1().iter_mut() {
            *vis = if system_id.0 == current_system.0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Dynamically adjusts star glare intensity/opacity based on camera distance (LOD).
/// When zoomed out, the glare is full brightness, hiding the surface.
/// When zoomed in close, the glare fades to transparent, revealing the star surface.
///
/// LOD distances scale proportionally with the star's visual radius so
/// the transition feels consistent regardless of star size.
pub(super) fn update_star_glare_lod(
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut glare_query: Query<(&GlobalTransform, &MeshMaterial3d<StarGlowMaterial>, &StarGlare)>,
    mut materials: ResMut<Assets<StarGlowMaterial>>,
) {
    if let Ok(cam_transform) = camera_query.single() {
        let cam_pos = cam_transform.translation();

        for (glare_transform, mat_handle, glare_data) in glare_query.iter_mut() {
            let glare_pos = glare_transform.translation();
            let distance = (cam_pos - glare_pos).length();

            // Scale distances by the star's visual radius so the fade
            // happens the same number of "star radii" away for any star size.
            // Multipliers derived from the original Sun-calibrated values
            // (min=200, max=1500) divided by Sun visual_radius ~104:
            //   min ≈ 2.0×, max ≈ 14.5× visual_radius
            let r = glare_data.visual_radius.max(1.0);
            let min_dist = r * 2.0;   // fully transparent (surface visible)
            let max_dist = r * 14.5;  // fully opaque (glow dominates)

            let t = ((distance - min_dist) / (max_dist - min_dist)).clamp(0.0, 1.0);
            let t_eased = t * t * (3.0 - 2.0 * t); // smoothstep

            if let Some(material) = materials.get_mut(mat_handle) {
                material.color_core = glare_data.base_core_color * t_eased;
                material.color_halo = glare_data.base_halo_color * t_eased;
            }
        }
    }
}

/// Fades the diffraction spike billboard in when far from the star and out when close.
/// The spikes are a long-range effect; at close range the surface limb-darkening takes over.
///
/// Distances scale with the star's visual radius for size-consistent behaviour.
pub(super) fn update_star_diffraction_lod(
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut diffraction_query: Query<(&GlobalTransform, &MeshMaterial3d<StarDiffractionMaterial>, &StarDiffraction)>,
    mut materials: ResMut<Assets<StarDiffractionMaterial>>,
) {
    if let Ok(cam_transform) = camera_query.single() {
        let cam_pos = cam_transform.translation();

        for (diff_transform, mat_handle, diff_data) in diffraction_query.iter_mut() {
            let diff_pos = diff_transform.translation();
            let dist = (cam_pos - diff_pos).length();

            // Scale by visual radius (original Sun-calibrated: min=400, max=2000 / 104 ≈ 3.85, 19.2)
            let r = diff_data.visual_radius.max(1.0);
            let min_dist = r * 4.0;
            let max_dist = r * 19.0;
            let t = ((dist - min_dist) / (max_dist - min_dist)).clamp(0.0, 1.0);
            let t_eased = t * t;

            if let Some(mat) = materials.get_mut(mat_handle) {
                mat.color = diff_data.base_color * t_eased;
            }
        }
    }
}

/// Push real elapsed time into every `StarGlowMaterial` so the shader can
/// animate its FBM corona and ray patterns. Uses `Time<Real>` (wall clock)
/// so animation speed is independent of game speed.
pub(super) fn update_glow_time(
    time: Res<Time<Real>>,
    mut glow_materials: ResMut<Assets<StarGlowMaterial>>,
) {
    let t = time.elapsed_secs();
    for (_id, mat) in glow_materials.iter_mut() {
        mat.time_phase = t;
    }
}

/// Push real elapsed time into every 3D corona/halo material.
pub(super) fn update_corona_3d_time(
    time: Res<Time<Real>>,
    mut corona_materials: ResMut<Assets<StarCorona3dMaterial>>,
    mut halo_materials: ResMut<Assets<StarHalo3dMaterial>>,
) {
    let t = time.elapsed_secs();
    for (_id, mat) in corona_materials.iter_mut() {
        mat.time_phase = t;
    }
    for (_id, mat) in halo_materials.iter_mut() {
        mat.time_phase = t;
    }
}

/// LOD system for 3D volumetric corona and halo shells.
///
/// Both shells fade in at distance and out when the camera is close
/// to the star surface, mirroring the behaviour of the billboard-based
/// glare LOD so the limb-darkened sphere is visible up close.
pub(super) fn update_star_corona_3d_lod(
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    corona_query: Query<(&GlobalTransform, &MeshMaterial3d<StarCorona3dMaterial>, &StarCoronaShell)>,
    halo_query: Query<(&GlobalTransform, &MeshMaterial3d<StarHalo3dMaterial>, &StarHaloShell)>,
    mut corona_materials: ResMut<Assets<StarCorona3dMaterial>>,
    mut halo_materials: ResMut<Assets<StarHalo3dMaterial>>,
) {
    if let Ok(cam_transform) = camera_query.single() {
        let cam_pos = cam_transform.translation();

        for (shell_transform, mat_handle, data) in corona_query.iter() {
            let shell_pos = shell_transform.translation();
            let distance = (cam_pos - shell_pos).length();
            let r = data.visual_radius.max(1.0);

            // Inner corona: fade in from 1.5× to 6× visual radius
            let min_dist = r * 1.5;
            let max_dist = r * 6.0;
            let t = ((distance - min_dist) / (max_dist - min_dist)).clamp(0.0, 1.0);
            let t_eased = t * t * (3.0 - 2.0 * t); // smoothstep

            if let Some(material) = corona_materials.get_mut(mat_handle) {
                material.color_core = data.base_core_color * t_eased;
                material.color_halo = data.base_halo_color * t_eased;
            }
        }

        for (shell_transform, mat_handle, data) in halo_query.iter() {
            let shell_pos = shell_transform.translation();
            let distance = (cam_pos - shell_pos).length();
            let r = data.visual_radius.max(1.0);

            // Outer halo: fade in from 3× to 10× visual radius
            let min_dist = r * 3.0;
            let max_dist = r * 10.0;
            let t = ((distance - min_dist) / (max_dist - min_dist)).clamp(0.0, 1.0);
            let t_eased = t * t * (3.0 - 2.0 * t); // smoothstep

            if let Some(material) = halo_materials.get_mut(mat_handle) {
                material.color_halo = data.base_halo_color * t_eased;
            }
        }
    }
}
