use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::{
    CurrentStarSystem, FloatingOrigin, KeplerOrbit, LagrangePointMarkers, LastLpClick,
    LocalOrbitAmplification, LpMarkerInfo, OrbitCenter, Selected, SpaceCoordinates, SystemId,
};
use super::systems::SCALING_FACTOR;
use crate::fleets::orbital_mechanics::G_CONST as ORBIT_G;
use crate::game_state::ActiveMenu;
use crate::plugins::camera::{CameraAnchor, EguiPanelBounds, GameCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, LogicalParent, Moon};

/// Approximate solar mass (kg) used for Hill-sphere and L-point calculations.
const SOLAR_MASS_KG: f64 = 1.989e30;

fn absolute_star_planet_lp_positions(
    host_star_pos: DVec3,
    planet_pos: DVec3,
    orbital_radius_au: f64,
    hill_radius_au: f64,
) -> Option<[DVec3; 5]> {
    let rel = planet_pos - host_star_pos;
    let rel_mag = rel.length();
    if rel_mag < 1e-10 {
        return None;
    }

    let rel_dir = rel / rel_mag;
    let cos60 = (std::f64::consts::PI / 3.0).cos();
    let sin60 = (std::f64::consts::PI / 3.0).sin();
    let (px, py, pz) = (rel_dir.x, rel_dir.y, rel_dir.z);

    Some([
        host_star_pos + rel_dir * (orbital_radius_au - hill_radius_au),
        host_star_pos + rel_dir * (orbital_radius_au + hill_radius_au),
        host_star_pos - rel_dir * orbital_radius_au,
        host_star_pos
            + DVec3::new(
                orbital_radius_au * (px * cos60 - py * sin60),
                orbital_radius_au * (px * sin60 + py * cos60),
                pz * orbital_radius_au,
            ),
        host_star_pos
            + DVec3::new(
                orbital_radius_au * (px * cos60 + py * sin60),
                orbital_radius_au * (-px * sin60 + py * cos60),
                pz * orbital_radius_au,
            ),
    ])
}

/// Draws blue Lagrange-point orbit rings and point markers for the currently
/// **anchored** body. If no anchor exists, falls back to the selected body.
///
/// Anchoring takes priority over selection - if a body is anchored (double-clicked),
/// its Lagrange points will be shown. If there's no anchor but a body is selected,
/// that body's Lagrange points are shown.
///
/// * **Planet/GasGiant/DwarfPlanet anchored** → draw the 5 Sun–Planet Lagrange rings.
/// * **Moon anchored** → draw the 5 Planet–Moon Lagrange rings.
pub fn draw_lagrange_point_rings(
    mut gizmos: Gizmos,
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    camera_query: Query<&CameraAnchor, With<GameCamera>>,
    selected_bodies: Query<Entity, With<Selected>>,
    body_query: Query<(
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
        Option<&SystemId>,
        Option<&Moon>,
        Option<&LocalOrbitAmplification>,
        Option<&OrbitCenter>,
    )>,
    floating_origin: Option<Res<FloatingOrigin>>,
    mut lp_markers: ResMut<LagrangePointMarkers>,
) {
    // Capture hover state before clearing for per-marker colour lookup.
    let hovered_index = lp_markers.hovered_index;
    // Refresh LP marker list every frame (populated below when in System view).
    lp_markers.markers.clear();

    if *view_mode != ViewMode::System {
        return;
    }

    // Check for anchor first (double-clicked body), then fall back to selection
    // This allows Lagrange points to be shown when either anchor OR selection exists
    let anchor_entity = camera_query.single().ok().and_then(|a| a.0);

    // Use anchor if available, otherwise use selection
    let Some(anchored) = anchor_entity.or_else(|| selected_bodies.iter().next()) else {
        return;
    };

    let Ok((
        anchored_body,
        anchored_sc,
        anchored_ko,
        anchored_parent,
        anchored_sys,
        is_moon,
        anchored_amp,
        anchored_orbit_center,
    )) = body_query.get(anchored)
    else {
        return;
    };

    // Only current system
    let body_system = anchored_sys.map(|s| s.0).unwrap_or(0);
    if body_system != current_system.0 {
        return;
    }

    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    // Convert a heliocentric AU position to render-space Vec3
    let to_render = |pos: DVec3| -> Vec3 {
        let s = (pos - origin_offset) * SCALING_FACTOR;
        Vec3::new(s.x as f32, s.y as f32, s.z as f32)
    };

    // Highlight color when this marker is hovered; default color otherwise.
    // (Uses the pre-captured `hovered_index` copy to avoid a borrow conflict
    // with the mutable `lp_markers.markers` push below.)
    let lp_color = |idx: usize| -> Color {
        if hovered_index == Some(idx) {
            Color::srgba(1.0, 1.0, 0.3, 1.0) // bright yellow when hovered
        } else {
            Color::srgba(0.50, 0.80, 1.0, 0.90) // default blue-cyan
        }
    };

    // Draw a small cross-dot marker at a render-space position.
    let draw_dot = |gizmos: &mut Gizmos, pos: Vec3, half: f32, color: Color| {
        gizmos.line(pos - Vec3::X * half, pos + Vec3::X * half, color);
        gizmos.line(pos - Vec3::Y * half, pos + Vec3::Y * half, color);
    };

    // ─────────────────────────────────────────────────────────────────────────
    // Case A: Planet/GasGiant/DwarfPlanet anchored → Star–Planet L-points
    // ─────────────────────────────────────────────────────────────────────────
    if is_moon.is_none()
        && matches!(
            anchored_body.body_type,
            crate::plugins::solar_system_data::BodyType::Planet
                | crate::plugins::solar_system_data::BodyType::GasGiant
                | crate::plugins::solar_system_data::BodyType::DwarfPlanet
        )
    {
        let Some(ko) = anchored_ko else { return };

        let a_au = ko.semi_major_axis;
        let m_planet = anchored_body.mass;

        let (host_star_pos, host_star_mass) = anchored_parent
            .and_then(|lp| body_query.get(lp.0).ok())
            .map(|(b, sc, _, _, _, _, _, _)| (sc.position, b.mass))
            .unwrap_or((DVec3::ZERO, SOLAR_MASS_KG));

        let r_hill = a_au * (m_planet / (3.0 * host_star_mass)).powf(1.0 / 3.0);
        // Host star GM used for LP transfer option metadata.
        let host_star_gm = ORBIT_G * host_star_mass;

        let p3d = anchored_sc.position;
        let Some(lp_positions) = absolute_star_planet_lp_positions(host_star_pos, p3d, a_au, r_hill)
        else {
            return;
        };
        let lp_radii: [f64; 5] = [a_au - r_hill, a_au + r_hill, a_au, a_au, a_au];

        // Minimum render-space distance from the planet's visual centre so that
        // LP markers (especially L1 on inner planets) don't appear inside the
        // enlarged visual sphere.
        let planet_render = to_render(p3d);
        let host_render = to_render(host_star_pos);
        let min_lp_dist = anchored_body.visual_radius * 1.6;
        let dot_half = (r_hill * SCALING_FACTOR * 0.10).clamp(5.0, 30.0) as f32;
        // Minimum 3D gap between L1 and L2.  Without this, both markers lie on
        // the planet–star axis and appear to stack when the camera is aligned
        // with that axis (e.g. when anchored to Earth and looking along the
        // ecliptic).
        let min_l1l2_sep = (min_lp_dist * 2.0).max(dot_half * 8.0);

        // Planet-star axis in render space (direction from host star toward planet).
        // This must be host-relative, not absolute, otherwise planets orbiting a
        // companion star in a binary system will wobble as the host star moves
        // around the barycenter. It also keeps the axis valid when the floating
        // origin is anchored to the planet and `planet_render` is near zero.
        let p_dir_render = (planet_render - host_render).normalize_or_zero();

        // ── Pass 1: clamp each LP outside the visual sphere ──────────────────
        let clamp_one = |pos_au: DVec3| -> Vec3 {
            let raw = to_render(pos_au);
            let from_planet = raw - planet_render;
            let d = from_planet.length();
            if d > 0.1 && d < min_lp_dist {
                planet_render + from_planet.normalize() * min_lp_dist
            } else {
                raw
            }
        };

        // L1 and L2 are computed directly from the planet-star axis rather than
        // going through `clamp_one`.  This avoids the precision loss that occurs
        // when r_hill is tiny in render space (d ≤ 0.1 → clamp skipped, or
        // near-zero `from_planet` → normalize gives a random direction).
        let r_hill_render = (r_hill * SCALING_FACTOR) as f32;
        let l1_dist = r_hill_render.max(min_lp_dist); // distance from planet render centre
        let l2_dist = r_hill_render.max(min_lp_dist);

        let mut render_positions: [Vec3; 5] = [
            planet_render - p_dir_render * l1_dist, // L1: toward star (inner)
            planet_render + p_dir_render * l2_dist, // L2: away from star (outer)
            clamp_one(lp_positions[2]),
            clamp_one(lp_positions[3]),
            clamp_one(lp_positions[4]),
        ];

        // ── Pass 2: ensure L1 and L2 are visually distinct ───────────────────
        // Enforce a minimum separation so they never overlap from any viewing
        // direction.
        {
            let axis = render_positions[1] - render_positions[0]; // L2 − L1
            let sep = axis.length();
            if sep < min_l1l2_sep {
                // Always push along the planet-star axis for L1/L2.
                // The previous perpendicular fallback (-pf.y, pf.x) was incorrect
                // and caused markers to jump to a tangential position.
                let push_dir = if sep > 0.001 {
                    axis.normalize()
                } else {
                    p_dir_render
                };
                let extra = (min_l1l2_sep - sep) * 0.5;
                render_positions[0] -= push_dir * extra; // push L1 inward (toward star)
                render_positions[1] += push_dir * extra; // push L2 outward (away from star)
            }
        }

        // ── Pass 3: draw and register ─────────────────────────────────────────
        let base_marker_count = lp_markers.markers.len();
        for (i, &render_pos) in render_positions.iter().enumerate() {
            let marker_idx = base_marker_count + i;
            let color = lp_color(marker_idx);
            draw_dot(&mut gizmos, render_pos, dot_half, color);
            gizmos.circle(
                bevy::math::Isometry3d::from_translation(render_pos),
                dot_half * 1.6,
                color,
            );

            // Record for hover / selection systems.
            lp_markers.markers.push(LpMarkerInfo {
                render_pos,
                hit_radius: dot_half * 3.0,
                point: (i + 1) as u8,
                planet_entity: anchored,
                planet_name: anchored_body.name.clone(),
                planet_sma_au: a_au,
                lp_radius_au: lp_radii[i],
                gm: host_star_gm,
            });
        }
    }
    // ─────────────────────────────────────────────────────────────────────────
    // Case B: Moon anchored → Planet–Moon L-points
    // ─────────────────────────────────────────────────────────────────────────
    else if is_moon.is_some() {
        let Some(ko) = anchored_ko else { return };
        let Some(parent_lp) = anchored_parent else {
            return;
        };

        let Ok((parent_body, parent_sc, _, _, _, _, _, _)) = body_query.get(parent_lp.0) else {
            return;
        };

        let a_moon = ko.semi_major_axis; // moon's SMA around its planet (AU)
        let m_planet = parent_body.mass;
        let m_moon = anchored_body.mass;

        if m_planet <= 0.0 || m_moon <= 0.0 {
            return;
        }

        let r_hill = a_moon * (m_moon / (3.0 * m_planet)).powf(1.0 / 3.0);

        // Visual amplification factor — moons may be rendered further from their
        // parent planet than raw AU physics to keep them visible.  LP markers must
        // use the same amplified scale so they appear at the correct visual position.
        let amp = anchored_amp.map(|a| a.0 as f64).unwrap_or(1.0);

        // Render-space center of the planet (marker reference point)
        let parent_render = to_render(parent_sc.position);

        // Moon's 3D orbital position around the planet.
        // Sol-system moons (no OrbitCenter) store their local orbital offset
        // in SpaceCoordinates directly; procedural moons (with OrbitCenter)
        // store absolute heliocentric position and need the parent subtracted.
        let moon_rel = if anchored_orbit_center.is_some() {
            anchored_sc.position - parent_sc.position
        } else {
            anchored_sc.position
        };
        let moon_dir = moon_rel.normalize_or_zero();

        // Use the moon's actual 3D direction for L-point orientation
        let lx = moon_dir.x;
        let ly = moon_dir.y;
        let lz = moon_dir.z;

        // L-point offsets from planet center in render units (amplified)
        // L1/L2 are along the moon-planet axis
        let l1_r = ((a_moon - r_hill) * amp * SCALING_FACTOR).max(10.0);
        let l2_r = ((a_moon + r_hill) * amp * SCALING_FACTOR).max(l1_r + 10.0);
        let sma_render = a_moon * amp * SCALING_FACTOR;

        // For L4/L5, we need to create triangular points in the orbital plane
        // Using the actual 3D moon direction
        let cos60 = 0.5;
        let sin60 = 0.8660254037845386;

        // LP heliocentric radii (for hover/select metadata)
        let lp_radii_b: [f64; 5] = [a_moon - r_hill, a_moon + r_hill, a_moon, a_moon, a_moon];

        // L-point offsets from planet center in 3D render units
        // L1/L2 are along the moon-planet axis
        let lp_offsets: [Vec3; 5] = [
            // L1: toward parent along moon direction
            Vec3::new((l1_r * lx) as f32, (l1_r * ly) as f32, (l1_r * lz) as f32),
            // L2: away from parent along moon direction
            Vec3::new((l2_r * lx) as f32, (l2_r * ly) as f32, (l2_r * lz) as f32),
            // L3: opposite to moon (around the parent)
            Vec3::new(
                (sma_render * -lx) as f32,
                (sma_render * -ly) as f32,
                (sma_render * -lz) as f32,
            ),
            // L4: +60 degrees in orbital plane
            Vec3::new(
                (sma_render * (lx * cos60 - ly * sin60)) as f32,
                (sma_render * (lx * sin60 + ly * cos60)) as f32,
                (sma_render * lz) as f32,
            ),
            // L5: -60 degrees in orbital plane
            Vec3::new(
                (sma_render * (lx * cos60 + ly * sin60)) as f32,
                (sma_render * (-lx * sin60 + ly * cos60)) as f32,
                (sma_render * lz) as f32,
            ),
        ];

        // Dot markers + circle for each L-point (no orbit rings drawn).
        // Use the amplified hill radius so markers scale with the visual orbit.
        let dot_half = (r_hill * amp * SCALING_FACTOR * 0.10).clamp(3.0, 15.0) as f32;
        let base_marker_count = lp_markers.markers.len();
        for (i, offset) in lp_offsets.iter().enumerate() {
            let render_pos = parent_render + *offset;
            let marker_idx = base_marker_count + i;
            let color = lp_color(marker_idx);
            draw_dot(&mut gizmos, render_pos, dot_half, color);
            gizmos.circle(
                bevy::math::Isometry3d::from_translation(render_pos),
                dot_half * 1.6,
                color,
            );

            // Record for hover detection (GM = parent planet's GM for moon LP).
            let planet_gm = ORBIT_G * m_planet;
            lp_markers.markers.push(LpMarkerInfo {
                render_pos,
                hit_radius: dot_half * 3.0,
                point: (i + 1) as u8,
                planet_entity: anchored,
                planet_name: anchored_body.name.clone(),
                planet_sma_au: a_moon,
                lp_radius_au: lp_radii_b[i],
                gm: planet_gm,
            });
        }
    }
}

/// Hover-detection system for Lagrange-point markers.
///
/// Tests the mouse ray against the LP marker hit-spheres recorded by
/// [`draw_lagrange_point_rings`] and updates
/// [`LagrangePointMarkers::hovered_index`].  Sets [`LastLpClick`]
/// when the player left-clicks on a hovered LP.
pub fn handle_lp_hover(
    view_mode: Res<ViewMode>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    mut lp_markers: ResMut<LagrangePointMarkers>,
    mut last_click: ResMut<LastLpClick>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut egui_contexts: bevy_egui::EguiContexts,
    active_menu: Res<ActiveMenu>,
    panel_bounds: Res<EguiPanelBounds>,
) {
    if *view_mode != ViewMode::System {
        lp_markers.hovered_index = None;
        return;
    }
    if active_menu.current.blocks_world_interaction() {
        lp_markers.hovered_index = None;
        return;
    }

    // Bail if egui is consuming the pointer.
    if let Ok(ctx) = egui_contexts.ctx_mut() {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.is_some_and(|p| !available.contains(p))
        } else {
            false
        };
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() || over_panel {
            lp_markers.hovered_index = None;
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        lp_markers.hovered_index = None;
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    // Find the nearest LP marker whose hit-sphere the ray passes through.
    let mut best: Option<(usize, f32)> = None; // (index, ray-projection distance)
    for (i, m) in lp_markers.markers.iter().enumerate() {
        let to_marker = m.render_pos - ray.origin;
        let proj = to_marker.dot(*ray.direction);
        if proj <= 0.0 {
            continue;
        }
        let closest = ray.origin + *ray.direction * proj;
        let dist = (m.render_pos - closest).length();
        if dist < m.hit_radius
            && best.is_none_or(|(_, prev_proj)| proj < prev_proj) {
                best = Some((i, proj));
            }
    }

    lp_markers.hovered_index = best.map(|(i, _)| i);

    // Set LastLpClick resource when the player clicks a hovered LP.
    if mouse_button.just_pressed(MouseButton::Left) {
        if let Some(idx) = lp_markers.hovered_index {
            let info = lp_markers.markers[idx].clone();
            last_click.info = Some(info);
        }
    } else if mouse_button.just_pressed(MouseButton::Right) {
        // TODO(lagrange-transfers): Re-enable right-click LP → transfer once Lagrange-point
        // transfer planning is working correctly. For now LP markers are display-only.
        // if let Some(idx) = lp_markers.hovered_index {
        //     if fleet_ui_state.selected_fleet.is_some() {
        //         let info = lp_markers.markers[idx].clone();
        //         fleet_ui_state.target_lagrange = Some(crate::ui::LagrangeTarget { ... });
        //         fleet_ui_state.show_transfer_popup = true;
        //     }
        // }
    }
}

#[cfg(test)]
mod tests {
    use super::absolute_star_planet_lp_positions;
    use bevy::math::DVec3;

    #[test]
    fn star_planet_lp_positions_are_host_relative() {
        let host_star_pos = DVec3::new(12.0, 4.0, 0.0);
        let planet_pos = DVec3::new(12.0, 5.0, 0.0);

        let positions = absolute_star_planet_lp_positions(host_star_pos, planet_pos, 1.0, 0.1)
            .expect("planet offset from host star should produce LP positions");

        assert!((positions[0] - DVec3::new(12.0, 4.9, 0.0)).length() < 1e-10);
        assert!((positions[1] - DVec3::new(12.0, 5.1, 0.0)).length() < 1e-10);
        assert!((positions[2] - DVec3::new(12.0, 3.0, 0.0)).length() < 1e-10);
    }

    #[test]
    fn star_planet_lp_positions_preserve_binary_star_offset() {
        let host_star_pos = DVec3::new(20.0, -3.0, 0.0);
        let planet_pos = DVec3::new(21.0, -3.0, 0.0);

        let positions = absolute_star_planet_lp_positions(host_star_pos, planet_pos, 1.0, 0.2)
            .expect("binary companion planet should produce LP positions");

        assert!((positions[0] - DVec3::new(20.8, -3.0, 0.0)).length() < 1e-10);
        assert!((positions[1] - DVec3::new(21.2, -3.0, 0.0)).length() < 1e-10);
        assert!((positions[2] - DVec3::new(19.0, -3.0, 0.0)).length() < 1e-10);
    }
}
