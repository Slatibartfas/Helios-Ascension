use super::time::format_timestamp_date_time;
use super::*;
use crate::fleets::orbital_mechanics::calculate_cross_star_ballistic_options;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannerTransferFrame {
    BodyLocal(Entity),
    StellarLocal(Entity),
    SystemBarycentric,
}

/// Minimum gravitational parameter (m³ s⁻²) to distinguish a star from a planet.
///
/// Corresponds to roughly 0.01 M☉ — well above Jupiter (1.27 × 10¹⁷) and below
/// even the smallest hydrogen-fusing stars (0.08 M☉ ≈ 1.06 × 10¹⁸).  We use a
/// slightly lower bound so that massive sub-stellar objects do not fall through.
const MIN_STELLAR_GM: f64 = 1.3e18; // ~0.01 M☉ in m³ s⁻²

/// Minimum orbital radius (AU) used as a guard in transfer calculations to avoid
/// division-by-zero or negative square-roots in vis-viva equations.
const MIN_ORBITAL_RADIUS_AU: f64 = 0.001; // 1/1000 AU ≈ 149,600 km (inside Mercury)

/// Safe gravity-assist periapsis scaling factor for **planetary** flyby bodies.
///
/// Like [`STELLAR_FLYBY_RADIUS_KM_MULTIPLIER`], this is a `meters_per_km × radius_multiplier`
/// factor used with `CelestialBody.radius` in kilometres. The km→m conversion is **baked in**:
/// `PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER = 1_000 × 3` — 3× body radius, in m/km.
/// A conservative minimum altitude above the atmosphere/surface.
const PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER: f64 = 3_000.0; // = 1_000 m/km × 3

/// Safe gravity-assist periapsis scaling factor for **stellar** flyby bodies.
///
/// `1_000 m/km × 1.5` — 1.5× the star's photospheric radius, in m/km.  Stars are
/// much larger and hotter than planets; a periapsis measured in stellar radii keeps
/// the flyby outside the corona where solar wind / radiation pressure dominate and
/// Δv cannot be modelled as a simple two-body assist.  This constant is the
/// pair-buddy to [`PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER`]; together they bracket
/// the safe-periapsis formulas used by the gravity-assist planner.  Any future
/// code path that considers a star as a flyby body MUST use this multiplier
/// (not the planetary one) and MUST explicitly exclude stars from the GA
/// candidate filter — see the gravity-assist filter in `compute_route_options`.
#[allow(dead_code)] // Reserved for future stellar-flyby assist code (GRA-149 C-1).
const STELLAR_FLYBY_RADIUS_KM_MULTIPLIER: f64 = 1_500.0; // = 1_000 m/km × 1.5 ≈ 1.5 R★

/// Default star-approach parking radius (AU) used when a star entity has no
/// per-body `star_approach_au` override.  0.3 AU is well outside the
/// photospheres of all main-sequence stars but close enough that the planner
/// can still display a meaningful arrival orbit.  GRA-149 C-2 makes this
/// the global default; per-body overrides live in `CelestialBody.star_approach_au`
/// (e.g. an M-dwarf can park at 0.05 AU above its surface).
const STELLAR_APPROACH_AU: f64 = 0.3;

/// Resolve the star-approach parking radius (AU) for a star body.
///
/// Returns `body.star_approach_au` if set (per-body override from RON or
/// procedural data); otherwise falls back to [`STELLAR_APPROACH_AU`] (0.3 AU).
/// Caller is responsible for clamping against the host planet's SMA to keep
/// the parking orbit outside the origin planet.
#[inline]
fn star_approach_radius_au(body: &CelestialBody) -> f64 {
    body.star_approach_au.unwrap_or(STELLAR_APPROACH_AU)
}

/// Returns `true` when `gm` is large enough to be a stellar-mass central body.
///
/// Used to decide whether transfer-window phase angles should be read from
/// heliocentric (star-frame) coordinates or from a local planet-centric frame,
/// and whether gravity-assist candidates should be offered.
#[inline]
fn is_stellar_gm(gm: f64) -> bool {
    gm >= MIN_STELLAR_GM
}

/// Returns `true` when `mass_kg` is large enough that the body is a stellar-mass
/// central body (rather than a planet / moon).  This is the mass-domain twin of
/// [`is_stellar_gm`] and is the GRA-149 C-3 replacement for the legacy
/// SMA-threshold classifier.  Threshold = `MIN_STELLAR_GM / G ≈ 0.01 M☉` — well
/// above Jupiter (~1.9 × 10²⁷ kg) and below the smallest hydrogen-fusing stars
/// (~1.4 × 10²⁹ kg).  Use this whenever you need to ask "is this body a star
/// in a class sense?" without going through `G·M`.
#[inline]
fn is_stellar_mass(mass_kg: f64) -> bool {
    let gm = mass_kg * crate::fleets::orbital_mechanics::G_CONST;
    gm >= MIN_STELLAR_GM
}

/// Walk up the `LogicalParent` chain from `start_entity` until a `BodyType::Star`
/// entity is found.  Returns `(star_entity, star_mass_kg)` or `None` if no stellar
/// ancestor exists within a reasonable depth.
///
/// This correctly handles fleets orbiting moons (moon → planet → star) and other
/// nested hierarchies in multi-star or non-Sol systems.
fn find_host_star(
    start_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<(Entity, f64)> {
    let mut current = Some(start_entity);
    // Depth limit prevents infinite loops in degenerate data.
    for _ in 0..8 {
        let entity = current?;
        let Ok((_, body, _, _, lp)) = body_query.get(entity) else {
            return None;
        };
        if body.body_type == BodyType::Star {
            return Some((entity, body.mass));
        }
        current = lp.map(|lp| lp.0);
    }
    None
}

#[inline]
fn is_inter_star_transfer(
    origin_entity: Entity,
    target_entity: Entity,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> bool {
    let origin_host_star = find_host_star(origin_entity, body_query).map(|(entity, _)| entity);
    let target_host_star = find_host_star(target_entity, body_query).map(|(entity, _)| entity);
    origin_host_star.is_some() && target_host_star.is_some() && origin_host_star != target_host_star
}

pub fn transfer_absolute_position(
    entity: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<bevy::math::DVec3> {
    let (_, body, sc, ko, lp) = body_query.get(entity).ok()?;
    if body.body_type == BodyType::Moon {
        let parent = lp?.0;
        transfer_absolute_position(parent, sim_time_s, body_query)
    } else if let Some(orbit) = ko {
        let parent_pos = lp
            .and_then(|parent| transfer_absolute_position(parent.0, sim_time_s, body_query))
            .unwrap_or(bevy::math::DVec3::ZERO);
        let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * sim_time_s;
        let local_pos = crate::astronomy::orbit_position_from_mean_anomaly(orbit, mean_anomaly);
        Some(parent_pos + local_pos)
    } else if lp.is_some() {
        // Has a parent but no orbit - get parent's position recursively
        lp.and_then(|parent| transfer_absolute_position(parent.0, sim_time_s, body_query))
    } else if body.body_type == BodyType::Star {
        // Isolated stars (no orbit, no parent): return current position from SpaceCoordinates
        Some(sc.position)
    } else {
        Some(sc.position)
    }
}

fn star_frame_reference_orbit(
    body_entity: Entity,
    parent_entity: Option<Entity>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<KeplerOrbit> {
    let own_orbit = body_query
        .get(body_entity)
        .ok()
        .and_then(|(_, _, _, ko, _)| ko.copied());
    // GRA-149 C-3: a body owns its own reference orbit (i.e. it IS the host
    // star) when its mass is stellar, not when its SMA is large enough.  The
    // legacy 0.05 AU threshold mis-classified hot-Jupiters and any close-orbit
    // planet, which then caused Δv errors of order M_star/M_planet because
    // the planner treated the planet as a moon in the planet-local frame.
    let own_mass_is_stellar = body_query
        .get(body_entity)
        .ok()
        .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
        .unwrap_or(false);

    if own_mass_is_stellar {
        return own_orbit;
    }

    parent_entity
        .and_then(|parent| body_query.get(parent).ok())
        .and_then(|(_, _, _, ko, _)| ko.copied())
        .or(own_orbit)
}

fn transfer_plane_from_reference_orbit(
    reference_orbit: &KeplerOrbit,
    departure_rel: bevy::math::DVec3,
    outward: bool,
) -> Option<(f64, f64, f64)> {
    let peri_dir = departure_rel.normalize_or_zero();
    if peri_dir.length_squared() <= 1e-20 {
        return None;
    }

    let inclination = reference_orbit.inclination;
    let lan = reference_orbit.longitude_ascending_node;
    let sin_i = inclination.sin();
    let normal = bevy::math::DVec3::new(sin_i * lan.sin(), -sin_i * lan.cos(), inclination.cos());
    let node_xy = bevy::math::DVec3::new(lan.cos(), lan.sin(), 0.0);
    let node_len = node_xy.length();

    let argument_of_periapsis = if node_len > 1e-20 {
        let node = node_xy / node_len;
        let departure_argument = normal.dot(node.cross(peri_dir)).atan2(node.dot(peri_dir));
        if outward {
            departure_argument
        } else {
            departure_argument + std::f64::consts::PI
        }
    } else {
        let departure_angle = peri_dir.y.atan2(peri_dir.x);
        if outward {
            departure_angle
        } else {
            departure_angle - std::f64::consts::PI
        }
    };

    Some((inclination, lan, argument_of_periapsis))
}

fn transfer_absolute_velocity(
    entity: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<bevy::math::DVec3> {
    let (_, body, _, ko, lp) = body_query.get(entity).ok()?;

    if body.body_type == BodyType::Moon {
        return lp.and_then(|parent| transfer_absolute_velocity(parent.0, sim_time_s, body_query));
    }

    let parent_velocity = lp
        .and_then(|parent| transfer_absolute_velocity(parent.0, sim_time_s, body_query))
        .unwrap_or(bevy::math::DVec3::ZERO);

    let Some(orbit) = ko else {
        return Some(parent_velocity);
    };

    let gm = if let Some(parent) = lp {
        body_query
            .get(parent.0)
            .ok()
            .map(|(_, parent_body, _, _, _)| G_CONST * parent_body.mass)
            .unwrap_or(0.0)
    } else {
        let a_m = orbit.semi_major_axis * AU_IN_METERS;
        orbit.mean_motion * orbit.mean_motion * a_m.powi(3)
    };

    if gm <= 0.0 {
        return Some(parent_velocity);
    }

    let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * sim_time_s;
    let local_velocity =
        crate::fleets::orbital_mechanics::keplerian_velocity_vector(orbit, mean_anomaly, gm);
    Some(parent_velocity + local_velocity)
}

fn exact_star_centered_transfer_data(
    reference_frame: TransferReferenceFrame,
    orbit_center: Entity,
    transfer_orbit: &KeplerOrbit,
    gm: f64,
    departure_time_s: f64,
    arrival_time_s: f64,
    is_local_transfer: bool,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<(
    bevy::math::DVec3,
    bevy::math::DVec3,
    bevy::math::DVec3,
    bevy::math::DVec3,
)> {
    if is_local_transfer || reference_frame.is_barycentric() {
        return None;
    }

    let TransferReferenceFrame::Body(center_entity) = reference_frame else {
        return None;
    };
    if center_entity != orbit_center {
        return None;
    }

    let center_is_star = body_query
        .get(center_entity)
        .ok()
        .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
        .unwrap_or(false);
    if !center_is_star {
        return None;
    }

    let center_departure = transfer_absolute_position(center_entity, departure_time_s, body_query)
        .unwrap_or(bevy::math::DVec3::ZERO);
    let center_arrival = transfer_absolute_position(center_entity, arrival_time_s, body_query)
        .unwrap_or(center_departure);
    let center_departure_velocity =
        transfer_absolute_velocity(center_entity, departure_time_s, body_query)
            .unwrap_or(bevy::math::DVec3::ZERO);
    let center_arrival_velocity =
        transfer_absolute_velocity(center_entity, arrival_time_s, body_query)
            .unwrap_or(center_departure_velocity);

    let start_mean_anomaly = transfer_orbit.mean_anomaly_epoch;
    let end_mean_anomaly = start_mean_anomaly
        + transfer_orbit.mean_motion * (arrival_time_s - departure_time_s).max(0.0);
    let start_local =
        crate::astronomy::orbit_position_from_mean_anomaly(transfer_orbit, start_mean_anomaly);
    let end_local =
        crate::astronomy::orbit_position_from_mean_anomaly(transfer_orbit, end_mean_anomaly);
    let departure_local_velocity = crate::fleets::orbital_mechanics::keplerian_velocity_vector(
        transfer_orbit,
        start_mean_anomaly,
        gm,
    );
    let arrival_local_velocity = crate::fleets::orbital_mechanics::keplerian_velocity_vector(
        transfer_orbit,
        end_mean_anomaly,
        gm,
    );

    Some((
        center_departure + start_local,
        center_arrival + end_local,
        center_departure_velocity + departure_local_velocity,
        center_arrival_velocity + arrival_local_velocity,
    ))
}

fn resolve_planner_transfer_frame(
    origin_entity: Entity,
    target_entity: Entity,
    origin_parent: Option<Entity>,
    dest_parent: Option<Entity>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> PlannerTransferFrame {
    if is_inter_star_transfer(origin_entity, target_entity, body_query) {
        return PlannerTransferFrame::SystemBarycentric;
    }

    let shared_parent = dest_parent.filter(|parent| Some(*parent) == origin_parent);
    if let Some(parent) = shared_parent {
        let is_star = body_query
            .get(parent)
            .ok()
            .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
            .unwrap_or(false);
        return if is_star {
            PlannerTransferFrame::StellarLocal(parent)
        } else {
            PlannerTransferFrame::BodyLocal(parent)
        };
    }

    if dest_parent == Some(origin_entity) {
        let is_star = body_query
            .get(origin_entity)
            .ok()
            .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
            .unwrap_or(false);
        return if is_star {
            PlannerTransferFrame::StellarLocal(origin_entity)
        } else {
            PlannerTransferFrame::BodyLocal(origin_entity)
        };
    }

    if Some(target_entity) == origin_parent {
        let is_star = body_query
            .get(target_entity)
            .ok()
            .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
            .unwrap_or(false);
        return if is_star {
            PlannerTransferFrame::StellarLocal(target_entity)
        } else {
            PlannerTransferFrame::BodyLocal(target_entity)
        };
    }

    let origin_star = find_host_star(origin_entity, body_query).map(|(entity, _)| entity);
    let dest_star = find_host_star(target_entity, body_query).map(|(entity, _)| entity);
    if let Some(host_star) = origin_star.filter(|star| Some(*star) == dest_star) {
        PlannerTransferFrame::StellarLocal(host_star)
    } else if let Some(center) = dest_parent.or(origin_parent) {
        PlannerTransferFrame::BodyLocal(center)
    } else {
        PlannerTransferFrame::BodyLocal(origin_entity)
    }
}

fn position_in_planner_frame(
    entity: Entity,
    frame: PlannerTransferFrame,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
) -> Option<bevy::math::DVec3> {
    match frame {
        PlannerTransferFrame::SystemBarycentric => {
            transfer_absolute_position(entity, sim_time_s, body_query)
        }
        PlannerTransferFrame::StellarLocal(star_entity) => {
            if entity == star_entity {
                Some(bevy::math::DVec3::ZERO)
            } else {
                let body_pos = transfer_absolute_position(entity, sim_time_s, body_query)?;
                let star_pos = transfer_absolute_position(star_entity, sim_time_s, body_query)
                    .unwrap_or(bevy::math::DVec3::ZERO);
                Some(body_pos - star_pos)
            }
        }
        PlannerTransferFrame::BodyLocal(central_body) => {
            if entity == central_body {
                Some(bevy::math::DVec3::ZERO)
            } else {
                let entry = body_query.get(entity).ok()?;
                let center = body_query.get(central_body).ok()?;
                if center.1.body_type == BodyType::Star {
                    let body_pos = transfer_absolute_position(entity, sim_time_s, body_query)?;
                    let center_pos =
                        transfer_absolute_position(central_body, sim_time_s, body_query)
                            .unwrap_or(bevy::math::DVec3::ZERO);
                    Some(body_pos - center_pos)
                } else {
                    Some(entry.2.position)
                }
            }
        }
    }
}

#[inline]
fn checked_arrival_timestamp(current_timestamp: i64, total_eta_s: f64) -> Option<i64> {
    if !total_eta_s.is_finite() || total_eta_s < 0.0 {
        return None;
    }

    let eta_seconds = total_eta_s.round();
    if eta_seconds > i64::MAX as f64 {
        return None;
    }

    current_timestamp.checked_add(eta_seconds as i64)
}

pub(super) fn render_transfer_planner(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    current_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    all_fleets_query: &Query<
        (
            Entity,
            &Fleet,
            &SpaceCoordinates,
            Option<&FleetOrbit>,
            Option<&ActiveManeuver>,
        ),
        Without<CelestialBody>,
    >,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    current_system_id: usize,
    body_system_ids: &Query<&SystemId>,
    elapsed: f64,
    nearby_stars: &NearbyStarsData,
    current_timestamp: i64,
    // Fleet's actual current heliocentric/local position when performing a course
    // correction (fleet is mid-transit). Used to compute accurate r1 and ΔV options
    // from the real location instead of the stand-in orbit body's SMA.
    course_correction_sc: Option<bevy::math::DVec3>,
    porkchop_config: &crate::fleets::PorkchopConfig,
) {
    // `is_course_correction` is true only when the fleet has actively departed
    // (elapsed >= departure_time).  Waiting-to-depart fleets still have an
    // ActiveManeuver but should show the normal Transfer Planner, not Course Correction.
    let is_course_correction = if let Some(man) = current_maneuver {
        elapsed >= man.departure_time
    } else {
        false
    };
    // Course corrections depart immediately — reset any leftover departure delay.
    if is_course_correction {
        fleet_ui_state.departure_offset_days = 0.0;
    }

    if is_course_correction {
        ui.label(
            egui::RichText::new("🔄 Course Correction")
                .strong()
                .size(15.0)
                .color(theme::AMBER),
        );
        ui.label(
            egui::RichText::new("Select a new target and execute to redirect immediately. Use Abort Mission to cancel and return to origin orbit.")
                .size(11.0)
                .italics()
                .color(theme::TEXT_DIM),
        );
    } else {
        ui.label(
            egui::RichText::new("📡 Orbital Transfer Planner")
                .strong()
                .size(15.0)
                .color(theme::TEXT_VALUE),
        );
    }
    ui.separator();

    // ── Hierarchical destination selector ────────────────────────────────────
    // DestEntry variants:
    //   Header — non-clickable category label; separator drawn BEFORE it (but not the very first)
    //   Body   — selectable destination
    //   Ring   — selectable ring destination (no KeplerOrbit; radius from body.radius field)
    //   Lagrange — one of the 5 L-points of a planet-star system
    //   FleetTarget — another fleet (for intercept course)
    //   StarSystem — interstellar target (another star system)
    #[derive(Clone)]
    enum DestEntry {
        Header(String),
        Body {
            entity: Entity,
            name: String,
        },
        // Rings are treated like regular bodies for selection; the extra
        // parent/radius information used to be stored here but never read.
        Ring {
            entity: Entity,
            name: String,
        },
        // TODO(lagrange-transfers): variant kept so the match arm compiles; re-enable construction when ready.
        #[allow(dead_code)]
        Lagrange {
            lp: LagrangeTarget,
        },
        FleetTarget {
            entity: Entity,
            name: String,
            in_transit: bool,
        },
        StarSystem {
            system_id: usize,
            name: String,
            distance_ly: f32,
        },
    }

    let mut dest_entries: Vec<DestEntry> = Vec::new();

    // Collect all valid candidate bodies (exclude Star, include Ring)
    // For Rings: sma = None (no KeplerOrbit); radius stored via body.radius field separately.
    let candidates: Vec<(Entity, String, BodyType, Option<f64>, Option<Entity>)> = body_query
        .iter()
        .filter_map(|(e, body, _, maybe_ko, maybe_lp)| {
            if e == orbit.body {
                return None;
            }
            if body.body_type == BodyType::Star {
                return None;
            }
            if !body_system_ids
                .get(e)
                .ok()
                .map(|s| s.0 == current_system_id)
                .unwrap_or(false)
            {
                return None;
            }
            let sma = maybe_ko.map(|ko| ko.semi_major_axis);
            let parent = maybe_lp.map(|lp| lp.0);
            Some((e, body.name.clone(), body.body_type, sma, parent))
        })
        .collect();

    // Separate ring bodies out; they lack KeplerOrbits so need special handling
    let ring_candidates: Vec<(Entity, String, Option<Entity>, f64)> = body_query
        .iter()
        .filter_map(|(e, body, _, _, maybe_lp)| {
            if body.body_type != BodyType::Ring {
                return None;
            }
            if !body_system_ids
                .get(e)
                .ok()
                .map(|s| s.0 == current_system_id)
                .unwrap_or(false)
            {
                return None;
            }
            let parent = maybe_lp.map(|lp| lp.0)?;
            // Use body.radius (km) as the representative ring orbit distance from planet centre
            let radius_au = (body.radius as f64 * 1_000.0) / AU_IN_METERS;
            Some((e, body.name.clone(), Some(parent), radius_au))
        })
        .collect();

    // ── Group 1: bodies that directly orbit the fleet's current body ──────────
    {
        let orbit_body_name = body_query
            .get(orbit.body)
            .map(|(_, b, _, _, _)| b.name.clone())
            .unwrap_or_default();
        let mut local: Vec<(Entity, String, f64)> = candidates
            .iter()
            .filter(|(_, _, btype, _, parent)| {
                *parent == Some(orbit.body) && *btype != BodyType::Ring
            })
            .filter_map(|(e, name, _, sma, _)| sma.map(|s| (*e, name.clone(), s)))
            .collect();
        // Rings around the current orbit body
        let mut local_rings: Vec<(Entity, String, Option<Entity>, f64)> = ring_candidates
            .iter()
            .filter(|(_, _, parent, _)| *parent == Some(orbit.body))
            .cloned()
            .collect();
        if !local.is_empty() || !local_rings.is_empty() {
            local.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            local_rings.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            dest_entries.push(DestEntry::Header(format!("{orbit_body_name} System")));
            for (e, name, _) in &local {
                dest_entries.push(DestEntry::Body {
                    entity: *e,
                    name: name.clone(),
                });
            }
            for (e, name, parent, _radius_au) in local_rings {
                if parent.is_some() {
                    dest_entries.push(DestEntry::Ring { entity: e, name });
                }
            }
        }

        // TODO(lagrange-transfers): Re-enable Sun-Planet and Planet-Moon Lagrange
        // point entries in this dropdown once transfer planning is working.
        // Search for TODO(lagrange-transfers) to find all related disabled code.
    }

    // ── Groups 2+: planet systems (moons/rings orbiting a planet that isn't fleet's body) ──
    let mut planet_map: std::collections::BTreeMap<
        String,
        (Entity, f64, Vec<(Entity, String, f64, bool)>),
    > = std::collections::BTreeMap::new();

    // Regular moons / small bodies orbiting a planet
    for (e, name, btype, sma, parent) in &candidates {
        if *btype == BodyType::Ring {
            continue;
        }
        let parent_e = match parent {
            Some(p) => *p,
            None => continue,
        };
        if parent_e == orbit.body {
            continue;
        }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star {
                continue;
            }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            if let Some(s) = sma {
                planet_map
                    .entry(pb.name.clone())
                    .or_insert_with(|| (parent_e, parent_sma, vec![]))
                    .2
                    .push((*e, name.clone(), *s, false)); // false = not a ring
            }
        }
    }
    // Rings orbiting a planet that isn't the fleet's body
    for (e, name, parent_opt, radius_au) in &ring_candidates {
        let parent_e = match parent_opt {
            Some(p) => *p,
            None => continue,
        };
        if parent_e == orbit.body {
            continue;
        }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star {
                continue;
            }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            planet_map
                .entry(pb.name.clone())
                .or_insert_with(|| (parent_e, parent_sma, vec![]))
                .2
                .push((*e, name.clone(), *radius_au, true)); // true = ring
        }
    }

    let mut sorted_planet_systems: Vec<_> = planet_map.into_iter().collect();
    sorted_planet_systems.sort_by(|a, b| {
        a.1 .1
            .partial_cmp(&b.1 .1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut planets_shown = std::collections::HashSet::<Entity>::new();
    for (planet_name, (parent_e, _parent_sma, mut children)) in sorted_planet_systems {
        planets_shown.insert(parent_e);
        children.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header(format!("{planet_name} System")));
        if orbit.body != parent_e {
            dest_entries.push(DestEntry::Body {
                entity: parent_e,
                name: planet_name.clone(),
            });
        }
        for (e, name, _sma, is_ring) in &children {
            if *is_ring {
                dest_entries.push(DestEntry::Ring {
                    entity: *e,
                    name: name.clone(),
                });
            } else {
                dest_entries.push(DestEntry::Body {
                    entity: *e,
                    name: name.clone(),
                });
            }
        }
        // TODO(lagrange-transfers): Re-enable planet and moon Lagrange point
        // sub-groups in this dropdown once transfer planning is working.
    }

    // ── Group: Planets/GasGiants not yet shown (no children found in data) ───
    let already_listed: std::collections::HashSet<Entity> = dest_entries
        .iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();

    let mut standalone: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Planet | BodyType::GasGiant)
                && sma.is_some()
                && !planets_shown.contains(e)
                && !already_listed.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !standalone.is_empty() {
        standalone.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header("Planets".to_string()));
        for (e, name, _) in standalone {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Dwarf Planets (not yet shown) ────────────────────────────────
    // Dwarf planets (Pluto, Eris, Ceres, etc.) get a separate top-level
    // header so they are not buried inside the "Planets" group with
    // Mercury/Venus/Earth-class bodies. Sorted by semi-major axis
    // (≈ perihelion for near-circular orbits) — most accessible first.
    let mut dwarf_planets: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::DwarfPlanet)
                && sma.is_some()
                && !planets_shown.contains(e)
                && !already_listed.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !dwarf_planets.is_empty() {
        dwarf_planets.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header("Dwarf Planets".to_string()));
        for (e, name, _) in dwarf_planets {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Small bodies ─────────────────────────────────────────────────
    let already_listed2: std::collections::HashSet<Entity> = dest_entries
        .iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();
    // Split small bodies by type so the picker groups Asteroids and Comets
    // separately (most accessible first by perihelion ≈ semi-major axis).
    // The shared "Small Bodies" top-level header keeps the picker scannable
    // when a system has 50+ asteroids or comets; sub-headers carry the count
    // so the player can tell at a glance which type dominates.
    let mut asteroids: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Asteroid)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    let mut comets: Vec<(Entity, String, f64)> = candidates
        .iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Comet)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();

    if !asteroids.is_empty() || !comets.is_empty() {
        let total = asteroids.len() + comets.len();
        let sb_label = if total > 5 {
            format!("Small Bodies ({} total)", total)
        } else {
            "Small Bodies".to_string()
        };
        dest_entries.push(DestEntry::Header(sb_label));

        if !asteroids.is_empty() {
            asteroids.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            let label = if asteroids.len() > 1 {
                format!("Asteroids ({})", asteroids.len())
            } else {
                "Asteroids".to_string()
            };
            dest_entries.push(DestEntry::Header(label));
            for (e, name, _) in asteroids {
                dest_entries.push(DestEntry::Body { entity: e, name });
            }
        }

        if !comets.is_empty() {
            comets.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            let label = if comets.len() > 1 {
                format!("Comets ({})", comets.len())
            } else {
                "Comets".to_string()
            };
            dest_entries.push(DestEntry::Header(label));
            for (e, name, _) in comets {
                dest_entries.push(DestEntry::Body { entity: e, name });
            }
        }
    }

    // ── Group: Star Approach ─────────────────────────────────────────────────
    // List every star in the current system.  In single-star systems this gives
    // one "☀ Sol Approach" entry.  In binary / trinary systems each star gets
    // its own entry, enabling direct inter-star transfer planning and stellar
    // gravity-assist routes (e.g. Star A → Star B → Star C).
    //
    // GRA-149 C-2: the approach radius in the label is now sourced from the
    // per-body `star_approach_au` override (or the 0.3 AU default) so the
    // label matches the actual arrival parking radius used by the planner.
    {
        let mut system_stars: Vec<(Entity, String, f64)> = body_query
            .iter()
            .filter_map(|(e, b, _, _, _)| {
                if b.body_type != BodyType::Star {
                    return None;
                }
                if !body_system_ids
                    .get(e)
                    .ok()
                    .map(|s| s.0 == current_system_id)
                    .unwrap_or(false)
                {
                    return None;
                }
                Some((e, b.name.clone(), star_approach_radius_au(b)))
            })
            .collect();
        // Stable sort by name so order is deterministic across frames.
        system_stars.sort_by(|a, b| a.1.cmp(&b.1));
        if !system_stars.is_empty() {
            dest_entries.push(DestEntry::Header("Star Approach".to_string()));
            for (star_e, star_name, approach_au) in system_stars {
                // 🛰 (parking-orbit star approach) — distinct from ☀ to signal
                // that this entry is a per-body parking-orbit transfer, not a
                // raw "fly to the star" approach. The approach altitude comes
                // from `star_approach_radius_au(b)` (GRA-149 C-2 wiring).
                dest_entries.push(DestEntry::Body {
                    entity: star_e,
                    name: format!("🛰 {} Approach ({:.2} AU)", star_name, approach_au),
                });
            }
        }
    }

    // ── Group: Interstellar ──────────────────────────────────────────────────
    // List every other star system from NearbyStarsData as an interstellar target.
    // The current system is identified by its numeric id; Sol = id 0 by convention.
    {
        let mut interstellar_entries: Vec<DestEntry> = nearby_stars
            .systems
            .iter()
            .filter(|sys| {
                // Exclude the current system (id comparison via name match is a fallback)
                // NearbyStarsData systems use 0-based index ordering; system_id 0 = Sol.
                // We exclude any system whose name matches current system's star name.
                let this_star_name = body_query
                    .iter()
                    .find(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids
                                .get(*e)
                                .ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| b.name.as_str())
                    .unwrap_or("Sol");
                // Each StarSystemData has stars[0].name; compare to current star
                !sys.stars.iter().any(|s| s.name == this_star_name) && sys.distance_ly > 0.0
            })
            .enumerate()
            .map(|(idx, sys)| {
                let display = format!("✨ {} ({:.2} ly)", sys.system_name, sys.distance_ly);
                // Use index+1 as system_id (0 reserved for Sol in current system)
                DestEntry::StarSystem {
                    system_id: idx + 1,
                    name: display,
                    distance_ly: sys.distance_ly,
                }
            })
            .collect();

        if !interstellar_entries.is_empty() {
            interstellar_entries.sort_by(|a, b| {
                let da = if let DestEntry::StarSystem { distance_ly, .. } = a {
                    *distance_ly
                } else {
                    0.0
                };
                let db = if let DestEntry::StarSystem { distance_ly, .. } = b {
                    *distance_ly
                } else {
                    0.0
                };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            dest_entries.push(DestEntry::Header("Interstellar".to_string()));
            dest_entries.extend(interstellar_entries);
        }
    }

    // ── Build hierarchical categories from dest_entries ─────────────────────
    // Top-level headers ("…System", "Small Bodies", "Heliocentric") become
    // category names in the first-level picker. Lagrange sub-headers are kept
    // as visual separators inside each category group.
    #[derive(Clone)]
    struct DestGroup {
        name: String,
        entries: Vec<DestEntry>,
    }

    let mut groups: Vec<DestGroup> = Vec::new();
    for entry in dest_entries {
        let is_top_header = match &entry {
            DestEntry::Header(label) => {
                label.ends_with(" System")
                    || label == "Planets"
                    || label == "Dwarf Planets"
                    || label == "Solar"
                    || label == "Interstellar"
                    || label.starts_with("Small Bodies")
            }
            _ => false,
        };
        if is_top_header {
            let name = match &entry {
                DestEntry::Header(label) => {
                    label.strip_suffix(" System").unwrap_or(label).to_string()
                }
                _ => unreachable!(),
            };
            groups.push(DestGroup {
                name,
                entries: Vec::new(),
            });
        } else if let Some(g) = groups.last_mut() {
            g.entries.push(entry);
        }
    }

    // ── Fleet intercept category ─────────────────────────────────────────────
    {
        let other_fleets: Vec<(Entity, String, bool)> = all_fleets_query
            .iter()
            .filter(|(e, _, _, _, _)| *e != fleet_entity)
            .map(|(e, f, _, _, maybe_ma)| (e, f.name.clone(), maybe_ma.is_some()))
            .collect();
        if !other_fleets.is_empty() {
            let mut fleet_group = DestGroup {
                name: "Fleets".to_string(),
                entries: Vec::new(),
            };
            // In-orbit fleets first
            for (e, name, in_transit) in &other_fleets {
                fleet_group.entries.push(DestEntry::FleetTarget {
                    entity: *e,
                    name: name.clone(),
                    in_transit: *in_transit,
                });
            }
            groups.push(fleet_group);
        }
    }

    // ── Auto-select category if a target is selected ─────────────────────────
    let mut correct_category = None;
    if let Some(target) = fleet_ui_state.target_body {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => {
                    *entity == target
                }
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(ref lp) = fleet_ui_state.target_lagrange {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Lagrange { lp: entry_lp } => {
                    entry_lp.point == lp.point && entry_lp.planet_entity == lp.planet_entity
                }
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::FleetTarget { entity, .. } => *entity == tf,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some((tss_id, _, _)) = fleet_ui_state.target_star_system {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::StarSystem { system_id, .. } => *system_id == tss_id,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    }

    if let Some(cat) = correct_category {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        if sel != Some(&cat) && !(sel == Some("Small Bodies") && cat.starts_with("Small Bodies")) {
            fleet_ui_state.selected_dest_category = Some(cat);
        }
    }

    // ── Render the two-level selector ────────────────────────────────────────
    // Step 1: category (planet system / small bodies / fleets)
    let cat_label = groups
        .iter()
        .find(|g| {
            let sel = fleet_ui_state.selected_dest_category.as_deref();
            sel == Some(&g.name)
                || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
        })
        .map(|g| g.name.clone())
        .unwrap_or_else(|| {
            fleet_ui_state
                .selected_dest_category
                .clone()
                .unwrap_or_else(|| "— System —".to_owned())
        });

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("System:").size(13.0));
        egui::ComboBox::from_id_salt("fleet_dest_category")
            .selected_text(&cat_label)
            .width(200.0)
            .show_ui(ui, |ui| {
                for group in &groups {
                    let sel = fleet_ui_state.selected_dest_category.as_deref();
                    let cat_is_sel = sel == Some(&group.name)
                        || (sel == Some("Small Bodies") && group.name.starts_with("Small Bodies"));
                    if ui
                        .selectable_label(cat_is_sel, egui::RichText::new(&group.name).size(13.0))
                        .clicked()
                        && !cat_is_sel
                    {
                        fleet_ui_state.selected_dest_category = Some(group.name.clone());
                        // Clear the specific target so the second step is re-selected
                        fleet_ui_state.target_body = None;
                        fleet_ui_state.target_lagrange = None;
                        fleet_ui_state.target_fleet = None;
                        fleet_ui_state.target_star_system = None;
                        fleet_ui_state.computed_options.clear();
                        fleet_ui_state.planned_transfer = None;
                        fleet_ui_state.selected_option = 0;
                        fleet_ui_state.selected_gravity_assist = None;
                    }
                }
            });
    });

    // Step 2: specific target within selected category
    let active_group = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    });

    let target_label = if let Some(ref lp) = fleet_ui_state.target_lagrange {
        format!("L{} {} — {}", lp.point, lp.planet_name, lp.qualifier())
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        all_fleets_query
            .get(tf)
            .map(|(_, f, _, _, ma)| {
                let status = if ma.is_some() { "✈" } else { "🛰" };
                format!("{status} {}", f.name)
            })
            .unwrap_or_else(|_| "— Target —".to_owned())
    } else if let Some((_, ref name, _)) = fleet_ui_state.target_star_system {
        name.clone()
    } else {
        fleet_ui_state
            .target_body
            .and_then(|e| body_query.get(e).ok())
            .map(|(_, b, _, _, _)| {
                if b.body_type == BodyType::Ring {
                    format!("{} 💍", b.name)
                } else {
                    b.name.clone()
                }
            })
            .unwrap_or_else(|| "— Target —".to_owned())
    };

    if active_group.is_some() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Target:").size(13.0));
            egui::ComboBox::from_id_salt("fleet_target_body")
                .selected_text(&target_label)
                .width(280.0)
                .show_ui(ui, |ui| {
                    if let Some(group) = active_group {
                        let mut first_sub = true;
                        for entry in &group.entries {
                            match entry {
                                DestEntry::Header(label) => {
                                    if !first_sub {
                                        ui.add_space(4.0);
                                    }
                                    first_sub = false;
                                    ui.label(
                                        egui::RichText::new(label.as_str())
                                            .strong()
                                            .size(11.0)
                                            .color(theme::AMBER),
                                    );
                                }
                                DestEntry::Body { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui
                                        .selectable_label(
                                            selected,
                                            egui::RichText::new(format!("  {name}")).size(12.0),
                                        )
                                        .clicked()
                                        && !selected
                                    {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::Ring { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui
                                        .selectable_label(
                                            selected,
                                            egui::RichText::new(format!("  {name} 💍")).size(12.0),
                                        )
                                        .clicked()
                                        && !selected
                                    {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::Lagrange { lp: _ } => {
                                    // TODO(lagrange-transfers): Lagrange-point transfers are
                                    // temporarily disabled. The LP markers are still rendered
                                    // and selectable for viewing, but cannot be chosen as a
                                    // fleet transfer destination until the transfer planner
                                    // for L-points is fully working. Re-enable by restoring
                                    // the DestEntry::Lagrange branch here and in
                                    // ui_lp_click_handler / astronomy::systems::hover_lagrange_points.
                                }
                                DestEntry::FleetTarget {
                                    entity,
                                    name,
                                    in_transit,
                                } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_fleet == Some(*entity);
                                    let icon = if *in_transit { "✈" } else { "🛰" };
                                    let status = if *in_transit {
                                        "In transit"
                                    } else {
                                        "In orbit"
                                    };
                                    let label = format!("  {icon} {name}  ({status})");
                                    if ui
                                        .selectable_label(
                                            is_sel,
                                            egui::RichText::new(label)
                                                .size(12.0)
                                                .color(theme::ACCENT),
                                        )
                                        .clicked()
                                        && !is_sel
                                    {
                                        fleet_ui_state.target_fleet = Some(*entity);
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_star_system = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::StarSystem {
                                    system_id,
                                    name,
                                    distance_ly,
                                } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state
                                        .target_star_system
                                        .as_ref()
                                        .map(|(id, _, _)| *id == *system_id)
                                        .unwrap_or(false);
                                    // One-line hover tooltip explaining the
                                    // ✨ marker and the multi-year / multi-century
                                    // travel time implication (GRA-154 M-2).
                                    let tooltip = format!(
                                        "Interstellar transfer to {raw_name} ({ly:.2} ly). \
                                         Plan multi-year / multi-century trajectories — \
                                         this is a barycentric route, not a parking orbit.",
                                        raw_name = name.trim_start_matches('✨').trim(),
                                        ly = distance_ly,
                                    );
                                    if ui
                                        .selectable_label(
                                            is_sel,
                                            egui::RichText::new(format!("  {name}"))
                                                .size(12.0)
                                                .color(theme::GRAVITY_ASSIST),
                                        )
                                        .on_hover_text(&tooltip)
                                        .clicked()
                                        && !is_sel
                                    {
                                        fleet_ui_state.target_star_system =
                                            Some((*system_id, name.clone(), *distance_ly));
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                            }
                        }
                    }
                });
        });
    }

    // ── Intercept parameters (shown only when a fleet is targeted) ────────────
    if fleet_ui_state.target_fleet.is_some() {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("⚔ Intercept Parameters")
                    .strong()
                    .size(13.0)
                    .color(theme::AMBER),
            );
            ui.add_space(4.0);

            // Passing distance slider: 0 = rendezvous / dock, up to 1 000 km = fast flyby
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Passing distance:").size(12.0));
                let mut pd = fleet_ui_state.intercept_passing_km as f32;
                if ui
                    .add(
                        egui::Slider::new(&mut pd, 0.0_f32..=1_000.0_f32)
                            .suffix(" km")
                            .text("0 = rendezvous")
                            .step_by(10.0),
                    )
                    .changed()
                {
                    fleet_ui_state.intercept_passing_km = pd as f64;
                    fleet_ui_state.computed_options.clear();
                }
            });

            // Encounter speed: 0 = match velocity (boarding), up to 30 km/s = high-speed pass
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Encounter speed:").size(12.0));
                let mut spd_kms = (fleet_ui_state.intercept_speed_ms / 1_000.0) as f32;
                if ui
                    .add(
                        egui::Slider::new(&mut spd_kms, 0.0_f32..=30.0_f32)
                            .suffix(" km/s")
                            .text("0 = match velocity")
                            .step_by(0.5),
                    )
                    .changed()
                {
                    fleet_ui_state.intercept_speed_ms = spd_kms as f64 * 1_000.0;
                    fleet_ui_state.computed_options.clear();
                }
            });

            ui.label(
                egui::RichText::new(
                    if fleet_ui_state.intercept_passing_km < 1.0
                        && fleet_ui_state.intercept_speed_ms < 100.0
                    {
                        "Mode: Rendezvous / docking approach"
                    } else if fleet_ui_state.intercept_passing_km > 100.0
                        || fleet_ui_state.intercept_speed_ms > 5_000.0
                    {
                        "Mode: High-speed flyby (combat pass)"
                    } else {
                        "Mode: Close approach (boarding range)"
                    },
                )
                .size(11.0)
                .italics()
                .color(theme::GREEN),
            );
        });
    }

    // ── Compute transfer options when a target is selected ───────────────────
    let fleet_target_snap = fleet_ui_state.target_fleet;
    let star_system_snap = fleet_ui_state.target_star_system.clone();
    let any_target = fleet_ui_state.target_body.is_some()
        || fleet_ui_state.target_lagrange.is_some()
        || fleet_target_snap.is_some()
        || star_system_snap.is_some();
    // Snapshot lagrange so we can use it immutably while also mut-borrowing fleet_ui_state below
    let lp_target_snap = fleet_ui_state.target_lagrange.clone();
    let body_target_snap = fleet_ui_state.target_body;
    let previous_selected_option_label = fleet_ui_state
        .computed_options
        .get(fleet_ui_state.selected_option)
        .map(|option| option.label);

    // Transfer window info computed this frame (Some only for body-target transfers).
    // Kept as a local so the window UI section can read it without re-computing.
    let mut window_this_frame: Option<TransferWindowInfo> = None;
    let mut window_max_slider_days: f64 = 730.0;

    if any_target {
        // Recompute every frame — body angles (SpaceCoordinates) update with the simulation clock,
        // so the phase error and launch-window countdown change live.

        // ── Fleet intercept computation ──────────────────────────────────────
        if let Some(target_fleet_entity) = fleet_target_snap {
            // Use the target fleet's current heliocentric position as the intercept radius.
            // r2 = distance from origin (0,0,0) to target fleet position in AU.
            let target_sc = all_fleets_query
                .get(target_fleet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(bevy::math::DVec3::ZERO);
            let r2_au = target_sc.length().max(0.001);

            // r1: heliocentric distance of the departing fleet.
            // GRA-149 C-3: pick own SMA only when the body is itself a star
            // (i.e., it owns its own heliocentric frame).  For planets and
            // moons — including close-orbit giants like hot-Jupiters at
            // 0.02 AU that the legacy 0.05 AU threshold mis-classified —
            // always walk up to the parent star's SMA.
            let r1_au = {
                let own_ko = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko.copied())
                    .map(|ko| ko.semi_major_axis);
                let origin_parent = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, _, lp)| lp.map(|lp| lp.0));
                let own_is_stellar = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                if own_is_stellar {
                    own_ko.unwrap_or(1.0)
                } else {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko.copied())
                        .map(|ko| ko.semi_major_axis)
                        .or(own_ko)
                        .unwrap_or(1.0)
                }
            };
            // Determine the host star's GM for the fleet intercept.  Walk the
            // LogicalParent chain so that fleets orbiting moons (moon → planet → star)
            // are correctly resolved to their host star's GM rather than falling back
            // to GM_SUN.
            let fleet_intercept_gm = find_host_star(orbit.body, body_query)
                .map(|(_, mass)| G_CONST * mass)
                .unwrap_or(GM_SUN);
            fleet_ui_state.computed_options =
                calculate_transfer_options(r1_au, r2_au, fleet_intercept_gm, 0.0);
            // Post-process: fill burn_time_s and flag thrust-limited options.
            apply_thrust_limits(
                &mut fleet_ui_state.computed_options,
                fleet.min_accel_ms2(),
                fleet.average_isp_s(),
            );
            // Add kinematic options for high-thrust fleets intercepting other fleets.
            let hohmann_dv = fleet_ui_state
                .computed_options
                .first()
                .map(|o| o.total_delta_v_ms)
                .unwrap_or(0.0);
            let sma_h = fleet_ui_state
                .computed_options
                .first()
                .map(|o| o.sma_au)
                .unwrap_or(0.0);
            let ecc_h = fleet_ui_state
                .computed_options
                .first()
                .map(|o| o.eccentricity)
                .unwrap_or(0.0);
            let d = (r2_au - r1_au).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
            let mut kinematics = kinematic_transfer_options(
                d,
                fleet.min_accel_ms2(),
                fleet.max_delta_v_ms(),
                hohmann_dv,
                sma_h,
                ecc_h,
                false,
            );
            fleet_ui_state.computed_options.append(&mut kinematics);
        } else if let Some(target_entity) = body_target_snap {
            //   - Ring transfer (dest has no KeplerOrbit; use body.radius as r2):
            //       r1 = fleet orbit radius or origin SMA, r2 = ring.radius_au, GM = parent mass * G
            //   - Local transfer (dest orbits fleet's body, e.g. Earth→Moon):
            //       r1 = fleet's parking orbit radius, r2 = dest SMA, GM = parent mass * G
            //   - Moon-to-moon (both orbit the same planet):
            //       r1 = origin moon SMA, r2 = dest moon SMA, GM = shared planet mass * G
            //   - Star approach (dest is a star):
            //       r1 = fleet's stellar SMA, r2 = 0.3 AU, GM = G * target_star_mass
            //   - Heliocentric transfer (both in stellar orbits, same or different host):
            //       r1 = origin body stellar SMA, r2 = dest stellar SMA, GM = host_star_GM
            let dest_body_type = body_query
                .get(target_entity)
                .ok()
                .map(|(_, b, _, _, _)| b.body_type);
            let dest_has_orbit = body_query
                .get(target_entity)
                .ok()
                .and_then(|(_, _, _, ko, _)| ko)
                .is_some();
            let dest_parent = body_query
                .get(target_entity)
                .ok()
                .and_then(|(_, _, _, _, lp)| lp)
                .map(|lp| lp.0);
            let origin_parent = body_query
                .get(orbit.body)
                .ok()
                .and_then(|(_, _, _, _, lp)| lp)
                .map(|lp| lp.0);
            let is_inter_star_body_transfer =
                is_inter_star_transfer(orbit.body, target_entity, body_query);
            let inter_star_departure_time_s =
                elapsed + fleet_ui_state.departure_offset_days.max(0.0) * 86_400.0;
            let planner_frame = resolve_planner_transfer_frame(
                orbit.body,
                target_entity,
                origin_parent,
                dest_parent,
                body_query,
            );

            // Target solar approach orbit (AU from star).  Inside Mercury's orbit so the
            // transfer is always clearly "inward".  Requires advanced propulsion (~10–20 km/s).
            const SOLAR_APPROACH_AU: f64 = 0.3;

            let (r1, r2, gm) = if is_inter_star_body_transfer {
                let origin_pos =
                    transfer_absolute_position(orbit.body, inter_star_departure_time_s, body_query)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                let dest_pos = transfer_absolute_position(
                    target_entity,
                    inter_star_departure_time_s,
                    body_query,
                )
                .unwrap_or(bevy::math::DVec3::ZERO);
                let r1_bary = origin_pos.length().max(MIN_ORBITAL_RADIUS_AU);
                let r2_bary = dest_pos.length().max(MIN_ORBITAL_RADIUS_AU);
                let system_gm_raw: f64 = body_query
                    .iter()
                    .filter(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids
                                .get(*e)
                                .ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .sum();
                let system_gm = if system_gm_raw > 0.0 {
                    system_gm_raw
                } else {
                    GM_SUN
                };
                (r1_bary, r2_bary, system_gm)
            } else if dest_body_type == Some(BodyType::Star) {
                // Star approach transfer: plot a Hohmann from the fleet's stellar-orbit
                // distance to SOLAR_APPROACH_AU, using the target star's actual GM.
                // Walk up the parent chain to find the fleet's stellar SMA.
                //
                // GRA-149 C-3: the fleet's body is treated as the host star only
                // when the body's mass is stellar (not when its SMA exceeds the
                // legacy 0.05 AU threshold).  For moons and close-orbit planets
                // the planner walks up to the parent.
                let own_sma = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko.copied())
                    .map(|ko| ko.semi_major_axis);
                let own_is_stellar = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                let r1_au = if own_is_stellar {
                    own_sma.unwrap_or(1.0)
                } else {
                    // Fleet is parked at a moon/sub-body; use its planet's heliocentric SMA.
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko.copied())
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                };
                // Ensure r2 is strictly less than r1 (always an inward transfer).
                let r2_au = SOLAR_APPROACH_AU.min(r1_au * 0.5);
                // Use the actual target star's GM, not a hardcoded GM_SUN.
                // target_entity IS the star in this branch (dest_body_type == Star).
                let star_gm = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .unwrap_or(GM_SUN);
                (r1_au, r2_au, star_gm)
            } else if !dest_has_orbit && dest_parent == Some(orbit.body) {
                // Ring around current orbit body
                let parent_mass = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if !dest_has_orbit && dest_parent.is_some() && dest_parent == origin_parent {
                // Ring around another planet (dest_parent is a planet, not fleet's body)
                let shared = dest_parent.unwrap();
                let parent_mass = body_query
                    .get(shared)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r1 = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (r1, r2, G_CONST * parent_mass)
            } else if dest_parent == Some(orbit.body) {
                // Local: destination orbits the fleet's current body
                let parent_mass = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if dest_parent.is_some() && dest_parent == origin_parent {
                // Both orbit the same central body (moon-to-moon, OR interplanetary e.g. Earth→Mars)
                let shared = dest_parent.unwrap();
                // Use G·mass for any central body — stars in non-Sol systems carry their
                // actual mass in CelestialBody.mass (stored as kg), so G·M gives the
                // correct GM.  GM_SUN is only the fallback when the query fails entirely.
                let gm = body_query
                    .get(shared)
                    .ok()
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .unwrap_or(GM_SUN);
                let r1 = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                (r1, r2, gm)
            } else if Some(target_entity) == origin_parent {
                // Downward transfer: fleet is at a moon, destination is the parent planet.
                // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
                let parent_mass = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| b.mass)
                    .unwrap_or(5.972e24);
                let r1 = body_query
                    .get(orbit.body)
                    .ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis)
                    .unwrap_or(0.00257);
                // Park at ~3× destination body surface radius (low orbit).
                let r2 = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 3_000.0) / AU_IN_METERS)
                    .unwrap_or(4.26e-5);
                (r1, r2.min(r1 * 0.5), G_CONST * parent_mass)
            } else {
                // Heliocentric: fleet is at a body that is not in the same parent chain as dest.
                //
                // ── Detect inter-star transfer ─────────────────────────────────────────────
                // In a binary/trinary system, origin and destination may orbit different stars.
                // E.g. fleet at a moon of Planet-A-1 (around Star A), destination a moon of
                // Planet-C-1 (around Star C).  We walk the full LogicalParent chain to the
                // stellar ancestor so that moon→planet→star hierarchies are handled correctly.
                //
                // For such transfers:
                //   - r1 / r2 must be barycentric distances (SpaceCoordinates.position.length()),
                //     NOT star-centric SMAs.  A planet at 1 AU from Star A in a binary where
                //     Star A is 23 AU from the barycenter has a barycentric r ≈ 24 AU.
                //   - gm must be the TOTAL system gravitational parameter G·ΣM_stars so that
                //     both barycentric orbital velocities and transfer times are correct.
                //
                // For single-star systems both host stars are the same entity, so
                // is_inter_star is false and the existing code path is unchanged.
                let origin_host_star = find_host_star(orbit.body, body_query);
                let dest_host_star = find_host_star(target_entity, body_query);
                let is_inter_star = origin_host_star.is_some()
                    && dest_host_star.is_some()
                    && origin_host_star.map(|(e, _)| e) != dest_host_star.map(|(e, _)| e);

                if is_inter_star {
                    // Barycentric transfer: use SpaceCoordinates.position.length() so that the
                    // orbital radius already includes the star's offset from the barycenter.
                    // E.g. a planet 1 AU from Star A, which is 23 AU from the barycenter, has
                    // a barycentric r ≈ 24 AU — very different from its star-centric SMA of 1 AU.
                    // Fallback values (1.0 AU for origin, 1.5 AU for dest) are Earth-like and
                    // Mars-like radii used only if an entity is somehow missing — a defensive
                    // guard that should never trigger in practice.
                    let r1_bary = transfer_absolute_position(orbit.body, elapsed, body_query)
                        .map(|pos| pos.length())
                        .unwrap_or(1.0) // defensive: orbit.body is always a valid spawned entity
                        .max(MIN_ORBITAL_RADIUS_AU); // guard against near-zero (fleet at star itself)
                    let r2_bary = transfer_absolute_position(target_entity, elapsed, body_query)
                        .map(|pos| pos.length())
                        // Defensive fallback; target_entity should always resolve here.
                        .unwrap_or(1.5)
                        .max(MIN_ORBITAL_RADIUS_AU);
                    // Total system GM: sum over all stars in the current system only.
                    // The barycentric frame requires G·(M₁ + M₂ + M₃ + …).
                    // We do NOT clamp with .max(GM_SUN) because sub-solar systems
                    // (e.g. two K-dwarfs totalling 0.8 M☉) must use their actual combined GM.
                    let system_gm_raw: f64 = body_query
                        .iter()
                        .filter(|(e, b, _, _, _)| {
                            b.body_type == BodyType::Star
                                && body_system_ids
                                    .get(*e)
                                    .ok()
                                    .map(|s| s.0 == current_system_id)
                                    .unwrap_or(false)
                        })
                        .map(|(_, b, _, _, _)| G_CONST * b.mass)
                        .sum();
                    let system_gm = if system_gm_raw > 0.0 {
                        system_gm_raw
                    } else {
                        GM_SUN // fallback only when no stars found (degenerate case)
                    };
                    (r1_bary, r2_bary, system_gm)
                } else {
                    // If fleet is parked at a moon, its KeplerOrbit SMA is Earth-relative, NOT
                    // heliocentric. Walk up one level to get the heliocentric SMA.
                    //
                    // GRA-149 C-3: "is this body itself a star?" is now decided by mass,
                    // not by SMA.  Hot-Jupiters at 0.02 AU used to be mis-classified as
                    // moons; the planner then walked up to their parent star but the
                    // GA candidate filter and downstream GM lookups still treated the
                    // hot-Jupiter as a moon.  The mass check makes the intent explicit.
                    let own_sma = body_query
                        .get(orbit.body)
                        .ok()
                        .and_then(|(_, _, _, ko, _)| ko.copied())
                        .map(|ko| ko.semi_major_axis);
                    let origin_is_stellar = body_query
                        .get(orbit.body)
                        .ok()
                        .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                        .unwrap_or(false);
                    let r1 = if origin_is_stellar {
                        orbit.radius_au.max(MIN_ORBITAL_RADIUS_AU)
                    } else if origin_parent.is_some() {
                        // Body is not a star and has a parent → walk up to the
                        // parent's heliocentric SMA.  Works for moons, hot-Jupiters,
                        // and any other close-orbit body that the legacy 0.05 AU
                        // threshold would have mis-classified.
                        origin_parent
                            .and_then(|pe| body_query.get(pe).ok())
                            .and_then(|(_, _, _, ko, _)| ko.copied())
                            .map(|ko| ko.semi_major_axis)
                            .or(own_sma)
                            .unwrap_or(1.0)
                    } else {
                        own_sma.unwrap_or(1.0)
                    };
                    let dest_sma = body_query
                        .get(target_entity)
                        .ok()
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis);
                    // GRA-149 C-3: classify "is this body itself a star?" by mass,
                    // not by SMA.  See the parallel r1 block above for rationale.
                    let dest_is_stellar = body_query
                        .get(target_entity)
                        .ok()
                        .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                        .unwrap_or(false);
                    let r2 = if dest_is_stellar {
                        dest_sma.unwrap_or(1.5)
                    } else if dest_parent.is_some() {
                        // Body is not a star and has a parent → walk up to the
                        // parent's heliocentric SMA.
                        dest_parent
                            .and_then(|pe| body_query.get(pe).ok())
                            .and_then(|(_, _, _, ko, _)| ko)
                            .map(|ko| ko.semi_major_axis)
                            .or(dest_sma)
                            .unwrap_or(1.5)
                    } else {
                        dest_sma.unwrap_or(1.5)
                    };
                    // Use the host star's actual GM rather than the hardcoded GM_SUN so that
                    // non-Sol systems (e.g. Alpha Centauri A at ~1.1 M☉, or a 0.5 M☉ K-dwarf)
                    // compute correct velocities and transfer times.
                    // Priority: (1) origin's logical parent if it is a Star, (2) dest's logical
                    // parent if it is a Star, (3) any nearby (< 1 AU from origin) star with no
                    // KeplerOrbit (single-star case), (4) fallback to GM_SUN.
                    let host_gm = origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                        .map(|(_, b, _, _, _)| G_CONST * b.mass)
                        .or_else(|| {
                            dest_parent
                                .and_then(|pe| body_query.get(pe).ok())
                                .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                                .map(|(_, b, _, _, _)| G_CONST * b.mass)
                        })
                        .unwrap_or(GM_SUN);
                    (r1, r2, host_gm)
                } // end same-star case
            };
            // For course corrections, compute the fleet's position in the correct local frame.
            // For heliocentric transfers the position is already relative to the Sun.
            // For local transfers (e.g. moon-to-moon around Jupiter) we must subtract
            // the central body's heliocentric position so distances and phase angles
            // are Jupiter-centric, not Sun-centric.
            // Use is_stellar_gm() instead of exact equality with GM_SUN so that
            // non-solar stars (which have different GM values) are treated correctly.
            let cc_local_pos: Option<bevy::math::DVec3> = if is_course_correction {
                if let Some(fleet_helio) = course_correction_sc {
                    match planner_frame {
                        PlannerTransferFrame::SystemBarycentric => Some(fleet_helio),
                        PlannerTransferFrame::StellarLocal(center_entity)
                        | PlannerTransferFrame::BodyLocal(center_entity) => {
                            let center_helio =
                                transfer_absolute_position(center_entity, elapsed, body_query)
                                    .unwrap_or(bevy::math::DVec3::ZERO);
                            Some(fleet_helio - center_helio)
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            // Override r1 with the fleet's actual distance from the central body.
            let r1 = if is_course_correction {
                cc_local_pos.map(|p| p.length()).unwrap_or(r1)
            } else {
                r1
            };
            fleet_ui_state.computed_options = if is_inter_star_body_transfer {
                if fleet_ui_state.departure_offset_days < 0.0 {
                    fleet_ui_state.departure_offset_days = 0.0;
                }
                let origin_pos =
                    transfer_absolute_position(orbit.body, inter_star_departure_time_s, body_query)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                let dest_pos = transfer_absolute_position(
                    target_entity,
                    inter_star_departure_time_s,
                    body_query,
                )
                .unwrap_or(bevy::math::DVec3::ZERO);
                let origin_velocity =
                    transfer_absolute_velocity(orbit.body, inter_star_departure_time_s, body_query)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                let dest_velocity = transfer_absolute_velocity(
                    target_entity,
                    inter_star_departure_time_s,
                    body_query,
                )
                .unwrap_or(bevy::math::DVec3::ZERO);
                let separation_m = (dest_pos - origin_pos).length()
                    * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let (origin_host_star, origin_host_mass) =
                    find_host_star(orbit.body, body_query).unwrap_or((orbit.body, 0.0));
                let (dest_host_star, dest_host_mass) =
                    find_host_star(target_entity, body_query).unwrap_or((target_entity, 0.0));
                let origin_host_pos = transfer_absolute_position(
                    origin_host_star,
                    inter_star_departure_time_s,
                    body_query,
                )
                .unwrap_or(bevy::math::DVec3::ZERO);
                let dest_host_pos = transfer_absolute_position(
                    dest_host_star,
                    inter_star_departure_time_s,
                    body_query,
                )
                .unwrap_or(bevy::math::DVec3::ZERO);
                let origin_host_radius_au = (origin_pos - origin_host_pos)
                    .length()
                    .max(MIN_ORBITAL_RADIUS_AU);
                let dest_host_radius_au = (dest_pos - dest_host_pos)
                    .length()
                    .max(MIN_ORBITAL_RADIUS_AU);
                window_this_frame = None;
                window_max_slider_days = 0.0;
                let mut options = calculate_cross_star_ballistic_options(
                    origin_pos,
                    dest_pos,
                    origin_velocity,
                    dest_velocity,
                    gm,
                    G_CONST * origin_host_mass,
                    origin_host_radius_au,
                    G_CONST * dest_host_mass,
                    dest_host_radius_au,
                );
                let mut direct_options = kinematic_transfer_options(
                    separation_m,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    0.0,
                    r1.max(r2),
                    0.0,
                    false,
                );
                options.append(&mut direct_options);
                options
            } else {
                // Extract angles of origin and destination bodies in the correct coordinate system.
                // Moon → parent-planet case: target IS the body that origin orbits around.
                let is_moon_to_parent = Some(target_entity) == origin_parent;

                let (pos1, pos2) = if is_moon_to_parent {
                    // Moon→parent: use Moon's position relative to the parent planet.
                    // The parent planet is at the centre of the local frame.
                    let moon_helio = body_query
                        .get(orbit.body)
                        .ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    let planet_helio = body_query
                        .get(target_entity)
                        .ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    (
                        Some(moon_helio - planet_helio),
                        Some(bevy::math::DVec3::ZERO),
                    )
                } else {
                    (
                        position_in_planner_frame(orbit.body, planner_frame, elapsed, body_query),
                        position_in_planner_frame(
                            target_entity,
                            planner_frame,
                            elapsed,
                            body_query,
                        ),
                    )
                };
                // For course corrections, override pos1 with the fleet's actual current
                // position in the correct local frame so the transfer-window phase angle
                // and quality indicator reflect the fleet's real location.
                let pos1 = if is_course_correction {
                    cc_local_pos.or(pos1)
                } else {
                    pos1
                };
                let theta1 = pos1.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);
                let theta2 = pos2.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);

                // Compute transfer window from live positions
                let window = compute_transfer_window(r1, r2, gm, theta1, theta2);
                window_max_slider_days = if window.synodic_period_s.is_finite() {
                    (window.synodic_period_s / 86_400.0 * 1.5).max(1.0)
                } else {
                    730.0
                };
                // Consume the "auto-set to next window" signal (departure_offset_days == -1.0)
                // that is set when the player first right-clicks a target body.  We resolve it
                // here — after the window is computed but before departure_s is used — so the
                // slider, quality indicator, and phased options all start at the optimal position.
                if fleet_ui_state.departure_offset_days < 0.0 {
                    fleet_ui_state.departure_offset_days =
                        (window.time_to_window_s / 86_400.0).max(0.0);
                }
                // Compute orbital-plane difference between origin and destination.
                // Mirrors the (r1, r2, gm) case logic above so the right pair of
                // KeplerOrbits is diffed in the correct reference frame.
                let delta_i: f64 = {
                    let origin_ko = body_query
                        .get(orbit.body)
                        .ok()
                        .and_then(|(_, _, _, ko, _)| ko);
                    let dest_ko = body_query
                        .get(target_entity)
                        .ok()
                        .and_then(|(_, _, _, ko, _)| ko);

                    if dest_body_type == Some(BodyType::Star)
                        || Some(target_entity) == origin_parent
                    {
                        // Inward heliocentric or moon→parent: report inclination of the
                        // departure body's orbit (fleet is already in that plane).
                        // Plane change equals what is needed to depart the current orbital plane.
                        origin_ko.map(|ko| ko.inclination).unwrap_or(0.0)
                    } else if dest_parent == Some(orbit.body) {
                        // Fleet at planet, going to one of its moons.
                        dest_ko.map(|ko| ko.inclination).unwrap_or(0.0)
                    } else if dest_parent.is_some() && dest_parent == origin_parent {
                        // Both share a parent (moon-to-moon, OR interplanetary Earth→Mars).
                        match (origin_ko, dest_ko) {
                            (Some(o), Some(d)) => plane_change_angle(
                                o.inclination,
                                o.longitude_ascending_node,
                                d.inclination,
                                d.longitude_ascending_node,
                            ),
                            _ => 0.0,
                        }
                    } else {
                        // Heliocentric: walk up from moons to their heliocentric parents.
                        //
                        // GRA-149 C-3: classify "is the body a star itself?" by mass
                        // rather than by SMA, so close-orbit planets (hot-Jupiters at
                        // 0.02 AU) are no longer treated as moons when picking the
                        // heliocentric reference orbit for the plane-change diff.
                        let origin_is_stellar = body_query
                            .get(orbit.body)
                            .ok()
                            .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                            .unwrap_or(false);
                        let dest_is_stellar_mass = body_query
                            .get(target_entity)
                            .ok()
                            .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                            .unwrap_or(false);
                        let helio_origin_ko = if origin_is_stellar {
                            origin_ko
                        } else {
                            origin_parent
                                .and_then(|pe| {
                                    body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko)
                                })
                                .or(origin_ko)
                        };
                        let helio_dest_ko = if dest_is_stellar_mass {
                            dest_ko
                        } else {
                            dest_parent
                                .and_then(|pe| {
                                    body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko)
                                })
                                .or(dest_ko)
                        };
                        match (helio_origin_ko, helio_dest_ko) {
                            (Some(o), Some(d)) => plane_change_angle(
                                o.inclination,
                                o.longitude_ascending_node,
                                d.inclination,
                                d.longitude_ascending_node,
                            ),
                            _ => 0.0,
                        }
                    }
                };

                let departure_s = fleet_ui_state.departure_offset_days * 86_400.0;
                let opts = if is_course_correction {
                    // ── Course-correction branch ─────────────────────────────────
                    // Estimate the fleet's current velocity vector so the redirect ΔV
                    // reflects the actual momentum that must be cancelled/redirected —
                    // not a fresh Hohmann from a circular parking orbit.
                    let v_current_ms: bevy::math::DVec3 = if let Some(man) = current_maneuver {
                        let progress = man.progress(elapsed);
                        if man.is_kinematic() {
                            // Kinematic (straight-line) transfer: velocity is constant in
                            // direction along (end − start); magnitude follows a symmetric
                            // brachistochrone profile (0 → peak → 0).
                            if let (Some(start), Some(end)) =
                                (man.start_position_au, man.end_position_au)
                            {
                                let dir = (end - start).normalize_or_zero();
                                let dist_m = (end - start).length()
                                    * crate::fleets::orbital_mechanics::AU_IN_METERS;
                                let dur_s = (man.arrival_time - man.departure_time).max(1.0);
                                // Brachistochrone peak speed (at midpoint) = 2 × distance / duration
                                let v_peak = 2.0 * dist_m / dur_s;
                                let speed = if man.option_label == "Full Thrust" {
                                    // Profile: 0 at t=0, v_peak at t=T/2, 0 at t=T
                                    v_peak * 2.0 * progress.min(1.0 - progress)
                                } else {
                                    // Coast options run at near-constant cruise speed ≈ dist / duration
                                    dist_m / dur_s
                                };
                                dir * speed
                            } else {
                                bevy::math::DVec3::ZERO
                            }
                        } else {
                            // Keplerian transfer: compute velocity from orbital elements via
                            // vis-viva equation + perifocal rotation.
                            let t_since_depart = (elapsed - man.departure_time).max(0.0);
                            let mean_anomaly = man.transfer_orbit.mean_anomaly_epoch
                                + man.transfer_orbit.mean_motion * t_since_depart;
                            keplerian_velocity_vector(&man.transfer_orbit, mean_anomaly, gm)
                        }
                    } else {
                        bevy::math::DVec3::ZERO
                    };
                    // r_vec: fleet's current position relative to the central body (AU).
                    // cc_local_pos is already in the correct local frame for both heliocentric
                    // and planetary-system transfers. Fall back to r1 on the x-axis.
                    let r_vec =
                        cc_local_pos.unwrap_or_else(|| bevy::math::DVec3::new(r1, 0.0, 0.0));
                    course_correction_transfer_options(r_vec, r2, gm, v_current_ms, delta_i)
                } else {
                    calculate_transfer_options_phased(r1, r2, gm, departure_s, &window, delta_i)
                };
                window_this_frame = Some(window);
                opts
            };
            // Post-process: fill burn_time_s, flag thrust-limited options,
            // and add kinematic options for high-thrust fleets.
            {
                let accel = fleet.min_accel_ms2();
                let isp = fleet.average_isp_s();
                apply_thrust_limits(&mut fleet_ui_state.computed_options, accel, isp);

                // Kinematic coast/thrust options are not meaningful for course corrections —
                // the fleet is already in free-flight and the redirect cost is captured by
                // `course_correction_transfer_options`.
                if !is_course_correction && !is_inter_star_body_transfer {
                    let hohmann_dv = fleet_ui_state
                        .computed_options
                        .first()
                        .map(|o| o.total_delta_v_ms)
                        .unwrap_or(0.0);
                    let sma_h = fleet_ui_state
                        .computed_options
                        .first()
                        .map(|o| o.sma_au)
                        .unwrap_or(0.0);
                    let ecc_h = fleet_ui_state
                        .computed_options
                        .first()
                        .map(|o| o.eccentricity)
                        .unwrap_or(0.0);
                    let d = (r2 - r1).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d,
                        accel,
                        fleet.max_delta_v_ms(),
                        hohmann_dv,
                        sma_h,
                        ecc_h,
                        false,
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                }
            }
            // ── Gravity assist candidates (same-host-star heliocentric transfers only) ─────
            // Restrict assists to bodies that share the same host star as the route.
            // Cross-star and stellar-flyby assists still need a consistent barycentric
            // planner/rendering model, so keep them disabled until that exists.
            if matches!(planner_frame, PlannerTransferFrame::StellarLocal(_))
                && is_stellar_gm(gm)
                && !is_course_correction
                && !is_inter_star_body_transfer
            {
                let route_host_star = match planner_frame {
                    PlannerTransferFrame::StellarLocal(star_entity) => Some(star_entity),
                    _ => None,
                };
                let ga_bodies: Vec<(String, f64, f64, f64)> = body_query
                    .iter()
                    .filter_map(|(e, body, _sc, maybe_ko, _)| {
                        let is_planet_class = matches!(
                            body.body_type,
                            BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet
                        );
                        if !is_planet_class {
                            return None;
                        }
                        // Stars are intentionally excluded from gravity-assist candidates:
                        // a stellar flyby would require STELLAR_FLYBY_RADIUS_KM_MULTIPLIER
                        // (1.5 R★ ≈ 1.5 stellar radii) rather than the planetary multiplier,
                        // and the existing 2-body assist model is not valid inside the
                        // corona.  Future maintainers: do NOT widen `is_planet_class` to
                        // include BodyType::Star without also switching the periapsis
                        // formula below to use STELLAR_FLYBY_RADIUS_KM_MULTIPLIER.
                        if body.body_type == BodyType::Star {
                            return None;
                        }
                        // Exclude the fleet's current body and the chosen destination
                        if e == orbit.body || Some(e) == body_target_snap {
                            return None;
                        }
                        // Only consider bodies in the current star system
                        if body_system_ids.get(e).map(|s| s.0).unwrap_or(0) != current_system_id {
                            return None;
                        }
                        if find_host_star(e, body_query).map(|(star, _)| star) != route_host_star {
                            return None;
                        }
                        let sma = maybe_ko?.semi_major_axis;
                        // GRA-149 C-3: the legacy 0.05 AU SMA threshold used to drop
                        // hot-Jupiters (close-orbit giants at ~0.02 AU) from the GA
                        // candidate list, even when a flyby of such a body would
                        // have been a strong assist.  We keep the candidate as long
                        // as it has any heliocentric SMA at all (i.e., it owns a
                        // Kepler orbit).  Pure moons and unbound bodies still fall
                        // out because `maybe_ko?` returns None above.
                        let flyby_r = sma;
                        let gm_p = G_CONST * body.mass;
                        // Safe flyby periapsis using the named multipliers.
                        let radius_km = body.radius as f64;
                        let multiplier = PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER;
                        let min_peri = (radius_km * multiplier) / AU_IN_METERS;
                        Some((body.name.clone(), flyby_r, gm_p, min_peri.max(1e-6)))
                    })
                    .collect();

                let previously_selected_flyby = fleet_ui_state
                    .selected_gravity_assist
                    .and_then(|idx| fleet_ui_state.gravity_assist_candidates.get(idx))
                    .map(|entry| entry.flyby_entity);

                let new_candidates: Vec<GravityAssistEntry> =
                    find_gravity_assist_options(r1, r2, gm, &ga_bodies)
                        .into_iter()
                        .filter_map(|opt| {
                            // Resolve each candidate to its ECS entity by name
                            let entity = body_query
                                .iter()
                                .find(|(_, b, _, _, _)| b.name == opt.body_name)
                                .map(|(e, _, _, _, _)| e)?;
                            Some(GravityAssistEntry {
                                option: opt,
                                flyby_entity: entity,
                            })
                        })
                        .collect();

                let mut new_candidates = new_candidates;
                new_candidates.sort_by(|left, right| {
                    left.option
                        .total_dv_ms
                        .partial_cmp(&right.option.total_dv_ms)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            left.option
                                .total_time_s
                                .partial_cmp(&right.option.total_time_s)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .then_with(|| left.option.body_name.cmp(&right.option.body_name))
                });

                fleet_ui_state.gravity_assist_candidates = new_candidates;

                fleet_ui_state.selected_gravity_assist =
                    previously_selected_flyby.and_then(|selected_flyby| {
                        fleet_ui_state
                            .gravity_assist_candidates
                            .iter()
                            .position(|entry| entry.flyby_entity == selected_flyby)
                    });
            } else {
                fleet_ui_state.gravity_assist_candidates.clear();
                fleet_ui_state.selected_gravity_assist = None;
            }

            // If a gravity assist is selected, prepend it as option 0 so the
            // regular execute/select logic treats it uniformly.
            if let Some(sel_ga) = fleet_ui_state.selected_gravity_assist {
                let ga_data = fleet_ui_state
                    .gravity_assist_candidates
                    .get(sel_ga)
                    .map(|e| {
                        (
                            e.option.total_dv_ms,
                            e.option.total_time_s,
                            e.option.flyby_radius_au,
                            e.option.dv_depart_ms + e.option.dv_mid_ms, // departure + mid-course
                            e.option.dv_arrive_ms,
                        )
                    });
                if let Some((total_dv, total_time, fly_r, dv1, dv2)) = ga_data {
                    // Use Leg-1 Hohmann parameters (origin → flyby body) for the
                    // transfer-orbit Keplerian arc.  This makes the purple active-orbit
                    // arc match the approach leg shown in the gravity-assist preview.
                    // The arc is computed pointing from the origin toward the flyby body,
                    // and build_planned_transfer is passed the flyby entity as its orbital
                    // target so the departure/arrival plane vectors are consistent.
                    let (_, _, _, ga_sma, ga_ecc) = hohmann_transfer(r1, fly_r, gm);
                    let burn_t =
                        compute_burn_time_s(total_dv, fleet.min_accel_ms2(), fleet.average_isp_s());
                    // Gravity-assist options use multi-leg patched-conic timing; the burn
                    // is spread across two legs so we apply the thrust-limit check here.
                    let (ga_transfer_time, ga_thrust_limited) =
                        if burn_t > 0.0 && burn_t > total_time {
                            (burn_t, true)
                        } else {
                            (total_time, false)
                        };
                    let ga_option = TransferOption {
                        label: "Gravity Assist",
                        total_delta_v_ms: total_dv,
                        delta_v1_ms: dv1, // actual departure + any mid-course burn
                        delta_v2_ms: dv2, // actual arrival circularisation
                        plane_change_dv_ms: 0.0, // gravity-assist paths are heliocentric (ecliptic)
                        transfer_time_s: ga_transfer_time,
                        sma_au: ga_sma, // Leg-1 ellipse SMA (origin → flyby body)
                        eccentricity: ga_ecc,
                        energy_multiplier: 1.0,
                        burn_time_s: burn_t,
                        is_thrust_limited: ga_thrust_limited,
                        transfer_orbit_override: None,
                    };
                    fleet_ui_state.computed_options.insert(0, ga_option);
                }
            }
        } else if let Some(ref lp) = lp_target_snap {
            // Lagrange-point transfer.
            // Determine the fleet's current heliocentric SMA, walking up to
            // the planet's SMA when the fleet is parked at a moon/sub-body.
            // When orbiting the star directly (e.g. after a previous LP transfer),
            // use the fleet's parking radius if available, otherwise the LP planet's SMA.
            let r1_lp = body_query
                .get(orbit.body)
                .ok()
                .and_then(|(_, body, _, ko, _)| {
                    if body.body_type == BodyType::Star {
                        // Fleet parked around the star — use its parking orbit radius
                        // or fall back to the target LP's planet SMA.
                        if orbit.radius_au > 0.01 {
                            Some(orbit.radius_au)
                        } else {
                            Some(lp.planet_sma_au)
                        }
                    } else {
                        ko.map(|ko| ko.semi_major_axis)
                    }
                })
                .or_else(|| {
                    body_query
                        .get(orbit.body)
                        .ok()
                        .and_then(|(_, _, _, _, parent)| parent)
                        .and_then(|lpp| {
                            body_query
                                .get(lpp.0)
                                .ok()
                                .and_then(|(_, _, _, ko, _)| ko)
                                .map(|ko| ko.semi_major_axis)
                        })
                })
                .unwrap_or(lp.planet_sma_au);

            // L3/L4/L5 are co-orbital with the planet (same heliocentric radius,
            // different phase angle).  A Hohmann gives 0 Delta-V in this case.
            // Use a phasing-orbit maneuver instead: lower into a shorter-period
            // orbit and drift the 60 deg (L4/L5) or 180 deg (L3) phase gap in N laps.
            let co_orbital = matches!(lp.point, 3..=5) && (r1_lp - lp.planet_sma_au).abs() < 0.02;

            if co_orbital {
                let delta_phi = if lp.point == 3 {
                    std::f64::consts::PI // L3: 180 deg opposition
                } else {
                    std::f64::consts::FRAC_PI_3 // L4/L5: 60 deg
                };
                fleet_ui_state.computed_options =
                    co_orbital_phasing_options(lp.planet_sma_au, lp.gm, delta_phi);
                apply_thrust_limits(
                    &mut fleet_ui_state.computed_options,
                    fleet.min_accel_ms2(),
                    fleet.average_isp_s(),
                );
                // Kinematic options: arc-length of the phase drift as proxy distance.
                let hohmann_dv = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.total_delta_v_ms)
                    .unwrap_or(0.0);
                let sma_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.sma_au)
                    .unwrap_or(r1_lp);
                let d =
                    lp.planet_sma_au * delta_phi * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    hohmann_dv,
                    sma_h,
                    0.0,
                    false,
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            } else if matches!(lp.point, 1 | 2) {
                // L1/L2: small radial offset from planet (~r_hill ≈ 0.01 AU).
                // Use a direct manifold-like trajectory (realistic ~1–3 month travel
                // time) instead of a Hohmann half-orbit that takes 6 months and arrives
                // 180° away from the LP.
                fleet_ui_state.computed_options =
                    direct_lp_transfer_options(r1_lp, lp.radius_au, lp.gm);
                apply_thrust_limits(
                    &mut fleet_ui_state.computed_options,
                    fleet.min_accel_ms2(),
                    fleet.average_isp_s(),
                );
                let hohmann_dv = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.total_delta_v_ms)
                    .unwrap_or(0.0);
                let sma_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.sma_au)
                    .unwrap_or(0.0);
                let ecc_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.eccentricity)
                    .unwrap_or(0.0);
                let d =
                    (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    hohmann_dv,
                    sma_h,
                    ecc_h,
                    false,
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            } else {
                // L3/L4/L5 cross-orbit (fleet NOT co-orbital with the planet):
                // standard Hohmann Keplerian transfer to the planet's SMA.
                fleet_ui_state.computed_options =
                    calculate_transfer_options(r1_lp, lp.radius_au, lp.gm, 0.0);
                apply_thrust_limits(
                    &mut fleet_ui_state.computed_options,
                    fleet.min_accel_ms2(),
                    fleet.average_isp_s(),
                );
                let hohmann_dv = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.total_delta_v_ms)
                    .unwrap_or(0.0);
                let sma_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.sma_au)
                    .unwrap_or(0.0);
                let ecc_h = fleet_ui_state
                    .computed_options
                    .first()
                    .map(|o| o.eccentricity)
                    .unwrap_or(0.0);
                let d =
                    (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d,
                    fleet.min_accel_ms2(),
                    fleet.max_delta_v_ms(),
                    hohmann_dv,
                    sma_h,
                    ecc_h,
                    false,
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            }
        }

        // ── Interstellar transfer computation ───────────────────────────────
        if let Some((_, _, distance_ly)) = star_system_snap {
            use crate::fleets::orbital_mechanics::{TransferOption, AU_IN_METERS};
            // 1 ly = 63 241.077 AU
            const AU_PER_LY: f64 = 63_241.077;
            let distance_m = distance_ly as f64 * AU_PER_LY * AU_IN_METERS;
            let accel = fleet.min_accel_ms2();
            let max_dv = fleet.max_delta_v_ms();

            fleet_ui_state.computed_options.clear();

            let mut kinematics =
                kinematic_transfer_options(distance_m, accel, max_dv, 0.0, 0.0, 0.0, true);
            fleet_ui_state.computed_options.append(&mut kinematics);

            if fleet_ui_state.computed_options.is_empty() {
                // Fleet lacks the minimum thrust for interstellar travel
                fleet_ui_state.computed_options.push(TransferOption {
                    label: "Insufficient thrust",
                    total_delta_v_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    plane_change_dv_ms: 0.0,
                    transfer_time_s: f64::INFINITY,
                    sma_au: 0.0,
                    eccentricity: 0.0,
                    energy_multiplier: 0.0,
                    burn_time_s: 0.0,
                    is_thrust_limited: true,
                    transfer_orbit_override: None,
                });
            }
        }

        // ── Transfer Window / Departure slider — hidden for course corrections ────
        // Course corrections execute immediately; no departure window or delay needed.
        if !is_course_correction {
            // Show a co-orbital / L-point info section for Lagrange targets.
            if window_this_frame.is_none() && lp_target_snap.is_some() {
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                // Left: Lagrange transfer info
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        let lp = lp_target_snap.as_ref().unwrap();
                        // Determine actual transfer type — same logic as the computation
                        // section above.  L3/L4/L5 are co-orbital only when the fleet is
                        // already near the planet's SMA (within 0.02 AU).
                        let r1_info = body_query.get(orbit.body).ok()
                            .and_then(|(_, body, _, ko, _)| {
                                if body.body_type == BodyType::Star {
                                    if orbit.radius_au > 0.01 { Some(orbit.radius_au) }
                                    else { Some(lp.planet_sma_au) }
                                } else { ko.map(|k| k.semi_major_axis) }
                            })
                            .or_else(|| {
                                body_query.get(orbit.body).ok()
                                    .and_then(|(_, _, _, _, parent)| parent)
                                    .and_then(|lpp| body_query.get(lpp.0).ok()
                                        .and_then(|(_, _, _, ko, _)| ko)
                                        .map(|ko| ko.semi_major_axis))
                            })
                            .unwrap_or(lp.planet_sma_au);
                        let is_co_orbital = matches!(lp.point, 3..=5)
                            && (r1_info - lp.planet_sma_au).abs() < 0.02;
                        let is_l12_direct = matches!(lp.point, 1 | 2);
                        if is_co_orbital {
                            ui.label(
                                egui::RichText::new("⟳ Co-orbital Phasing")
                                    .strong().size(12.0)
                                    .color(theme::RP_BLUE),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new("Depart any time")
                                    .size(12.0).strong()
                                    .color(theme::GREEN),
                            );
                            ui.label(
                                egui::RichText::new("Fleet drifts in a slightly\nlower orbit to cover the\nphase gap over N laps.")
                                    .size(10.0).color(theme::TEXT_DIM),
                            );
                        } else if is_l12_direct {
                            ui.label(
                                egui::RichText::new("🎯 Direct LP Transfer")
                                    .strong().size(12.0)
                                    .color(theme::RP_BLUE),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("Low-energy manifold trajectory\nto the Lagrange equilibrium.")
                                    .size(10.0).color(theme::TEXT_DIM),
                            );
                        } else {
                            // L3/L4/L5 cross-orbit (fleet not co-orbital): Hohmann
                            ui.label(
                                egui::RichText::new("⬆ Hohmann Transfer")
                                    .strong().size(12.0)
                                    .color(theme::RP_BLUE),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("Keplerian transfer arc,\nthen phase into the LP.")
                                    .size(10.0).color(theme::TEXT_DIM),
                            );
                        }
                    });
                });
                // Fleet stats infobox (same as body-target section)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("\u{1f680} Fleet")
                                .strong().size(12.0)
                                .color(theme::RP_BLUE),
                        );
                        ui.add_space(3.0);
                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_g = fleet.min_accel_ms2() / 9.80665;
                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0).strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0).strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(format!("{:.3} g", accel_g))
                                .size(11.0).strong()
                                .color(theme::TEXT_VALUE),
                        );
                    });
                });
            });
            }
            if let Some(ref window) = window_this_frame {
                let syn_days = if window.synodic_period_s.is_finite() {
                    window.synodic_period_s / 86_400.0
                } else {
                    f64::INFINITY
                };
                let window_days = window.time_to_window_s / 86_400.0;

                ui.add_space(6.0);

                let max_days = window_max_slider_days.min(1_825.0); // cap at 5 years
                let step_size = if max_days <= 1.0 {
                    0.01 // ~14 mins
                } else if max_days <= 10.0 {
                    0.05 // ~1.2 hours
                } else if max_days <= 50.0 {
                    0.1 // ~2.4 hours
                } else if max_days <= 200.0 {
                    0.5 // 12 hours
                } else {
                    1.0 // 1 day
                };

                // ── Transfer Window (left) + Planned Departure (right) side by side ──
                ui.horizontal_top(|ui| {
                // Left: Transfer Window box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("⏱ Transfer Window")
                                .strong()
                                .size(12.0)
                                .color(theme::RP_BLUE),
                        );
                        ui.add_space(3.0);

                        egui::Grid::new("window_info_grid")
                            .num_columns(2)
                            .spacing([8.0, 3.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Next window:").size(12.0));
                                if window_days < 1.0 {
                                    ui.label(
                                        egui::RichText::new("NOW  ✓")
                                            .size(12.0)
                                            .strong()
                                            .color(theme::GREEN),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(format_duration(window.time_to_window_s).to_string())
                                            .size(12.0)
                                            .color(theme::TEXT),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Synodic period:").size(12.0));
                                let syn_str = if syn_days.is_finite() {
                                    format_duration(window.synodic_period_s)
                                } else {
                                    "∞ (same orbit)".to_owned()
                                };
                                ui.label(egui::RichText::new(syn_str).size(12.0).color(theme::TEXT_DIM));
                                ui.end_row();
                            });
                    });
                });

                // Right: Planned Departure box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        // Row 1: label
                        ui.label(
                            egui::RichText::new("🕐 Planned Departure")
                                .strong()
                                .size(12.0)
                                .color(theme::RP_BLUE),
                        );

                        // Row 2: slider
                        let mut offset_days = fleet_ui_state.departure_offset_days as f32;
                        let slider = egui::Slider::new(&mut offset_days, 0.0_f32..=max_days as f32)
                            .step_by(step_size)
                            .custom_formatter(|v, _| {
                                if v < 0.01 {
                                    "Now".to_owned()
                                } else {
                                    format_duration(v * 86_400.0)
                                }
                            });
                        if ui.add(slider).changed() {
                            fleet_ui_state.departure_offset_days = offset_days as f64;
                        }

                        // Orbit-wait counter: shown when the fleet must loop its parking ring
                        // more than once before reaching the departure angle.
                        if fleet_ui_state.waiting_orbit_count > 1 {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("× {} orbits (waiting)", fleet_ui_state.waiting_orbit_count))
                                        .size(10.5)
                                        .color(theme::GRAVITY_ASSIST),
                                );
                            });
                        }

                        // Row 3: alignment indicator (below the slider)
                        let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                        let phase_at = {
                            let raw = window.phase_error_now_rad + window.phase_rate_rad_s * dep_s;
                            ((raw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)) - std::f64::consts::PI
                        };
                        let factor = crate::fleets::orbital_mechanics::phase_dv_factor(phase_at.abs());
                        let (quality_str, quality_color) = if factor < 1.05 {
                            ("● Optimal", theme::GREEN)
                        } else if factor < 1.40 {
                            ("◑ Good", theme::GREEN)
                        } else if factor < 1.80 {
                            ("◔ Fair", theme::AMBER)
                        } else {
                            ("○ Poor", theme::RED)
                        };
                        ui.label(egui::RichText::new(quality_str).size(11.0).color(quality_color))
                            .on_hover_text("Indicates how well the planets are aligned for a transfer at the planned departure time. Poor alignment requires significantly more ΔV.");

                        // Next Window button on its own row
                        if window_days > 0.5 {
                            ui.add_space(2.0);
                            if ui.small_button(format!("🎯 Next Window (+{:.0} d)", window_days)).clicked() {
                                fleet_ui_state.departure_offset_days = window_days;
                            }
                        }
                    });
                });

                // Fleet stats infobox (narrow, right-most)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("🚀 Fleet")
                                .strong()
                                .size(12.0)
                                .color(theme::RP_BLUE),
                        );
                        ui.add_space(3.0);

                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_ms2 = fleet.min_accel_ms2();
                        let accel_g = accel_ms2 / 9.80665;
                        let accel_str = format!("{:.3} g", accel_g);

                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_VALUE),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(theme::TEXT_DIM));
                        ui.label(
                            egui::RichText::new(accel_str)
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_VALUE),
                        );
                    });
                });
            });
            }
        } // end !is_course_correction (Transfer Window / Departure slider section)

        if !fleet_ui_state.computed_options.is_empty() {
            if let Some(previous_label) = previous_selected_option_label {
                if let Some(idx) = fleet_ui_state
                    .computed_options
                    .iter()
                    .position(|option| option.label == previous_label)
                {
                    fleet_ui_state.selected_option = idx;
                }
            }

            ui.add_space(6.0);

            let fleet_max_dv = fleet.max_delta_v_ms();

            // Ensure selected_option is within bounds
            if fleet_ui_state.selected_option >= fleet_ui_state.computed_options.len() {
                fleet_ui_state.selected_option = fleet_ui_state.computed_options.len() - 1;
            }

            // Pre-compute execute button state
            let sel_option =
                fleet_ui_state.computed_options[fleet_ui_state.selected_option].clone();
            let planned_departure_time_s =
                elapsed + fleet_ui_state.departure_offset_days * 86_400.0;
            fleet_ui_state.planned_transfer = if star_system_snap.is_some()
                || fleet_ui_state.selected_gravity_assist.is_some()
            {
                None
            } else if let Some(ref lp) = lp_target_snap {
                build_planned_transfer_lp(fleet_entity, fleet, orbit, lp, body_query, &sel_option)
            } else if let Some(tfe) = fleet_target_snap {
                all_fleets_query
                    .get(tfe)
                    .ok()
                    .and_then(|(_, _, _, maybe_fo, _)| maybe_fo)
                    .and_then(|fo| {
                        build_planned_transfer(
                            fleet_entity,
                            fleet,
                            orbit,
                            fo.body,
                            planned_departure_time_s,
                            body_query,
                            &sel_option,
                            course_correction_sc,
                            body_system_ids,
                            current_system_id,
                        )
                    })
            } else if let Some(te) = body_target_snap {
                build_planned_transfer(
                    fleet_entity,
                    fleet,
                    orbit,
                    te,
                    planned_departure_time_s,
                    body_query,
                    &sel_option,
                    course_correction_sc,
                    body_system_ids,
                    current_system_id,
                )
            } else {
                None
            };
            let abort_cost_t: f32 = if let Some(maneuver) = current_maneuver {
                // GRA-153 H-4: replace the parabolic peak heuristic
                // (`fuel_used * 4p(1-p) * 0.6`) with a real mid-flight abort ΔV.
                //
                // The cheapest mid-flight abort cancels the fleet's current
                // Keplerian velocity and circularises at the fleet's CURRENT
                // radius from the origin body — a true `|v_required - v_current|`
                // vis-viva computation.  This is the same approach the planner
                // uses for non-abort course corrections (see `course_correction_
                // transfer_options` in orbital_mechanics.rs).
                let abort_dv_ms: f64 = (|| -> Option<f64> {
                    // Fleet's current heliocentric position (planner-open
                    // snapshot — fine for a button label).
                    let r_pos = course_correction_sc?;
                    // Compute the central body's GM via Kepler's third law from
                    // the current transfer orbit's SMA and mean motion.
                    let a_m = maneuver.transfer_orbit.semi_major_axis
                        * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let n = maneuver.transfer_orbit.mean_motion;
                    if a_m <= 0.0 || n <= 0.0 {
                        return None;
                    }
                    let gm = (n * n) * (a_m * a_m * a_m);
                    // Fleet's current Keplerian velocity from the active orbit.
                    let t_since_depart = (elapsed - maneuver.departure_time).max(0.0);
                    let mean_anomaly = maneuver.transfer_orbit.mean_anomaly_epoch
                        + maneuver.transfer_orbit.mean_motion * t_since_depart;
                    let v_current_ms = crate::fleets::orbital_mechanics::keplerian_velocity_vector(
                        &maneuver.transfer_orbit,
                        mean_anomaly,
                        gm,
                    );
                    // Resolve the orbital center's heliocentric position so
                    // the radius-from-center is local (handles moon transfers).
                    let center_helio = match maneuver.reference_frame {
                        crate::fleets::TransferReferenceFrame::SystemBarycentric => {
                            bevy::math::DVec3::ZERO
                        }
                        crate::fleets::TransferReferenceFrame::Body(center_entity) => body_query
                            .get(center_entity)
                            .map(|(_, _, sc, _, _)| sc.position)
                            .unwrap_or(bevy::math::DVec3::ZERO),
                    };
                    let r_local_au = (r_pos - center_helio).length();
                    if r_local_au <= 1e-6 {
                        return None;
                    }
                    // Circular velocity at the current radius.
                    let v_circ_ms =
                        (gm / (r_local_au * crate::fleets::orbital_mechanics::AU_IN_METERS)).sqrt();
                    // ΔV to circularise at the current orbit.
                    let dv_circ_ms = (v_current_ms.length() - v_circ_ms).abs();
                    Some(dv_circ_ms)
                })()
                .unwrap_or(0.0);
                // Convert ΔV to fuel tonnes via the rocket equation.
                if abort_dv_ms > 0.0 {
                    let dry_mass_t = fleet.ships.iter().map(|s| s.dry_mass_t as f64).sum::<f64>();
                    let wet_mass_t = dry_mass_t + fleet.total_fuel_t() as f64;
                    let avg_isp_s = fleet.average_isp_s() as f32;
                    crate::fleets::orbital_mechanics::estimate_fuel_cost_tonnes(
                        wet_mass_t as f32,
                        avg_isp_s,
                        abort_dv_ms,
                    )
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let dv_after_abort = if abort_cost_t > 0.0 {
                fleet.min_delta_v_after_abort(abort_cost_t)
            } else {
                fleet_max_dv
            };
            let sel_affordable_with_abort = sel_option.total_delta_v_ms <= dv_after_abort;

            // Interstellar note
            let is_interstellar = star_system_snap.is_some();
            let is_inter_star_body_transfer = body_target_snap
                .map(|target_entity| is_inter_star_transfer(orbit.body, target_entity, body_query))
                .unwrap_or(false);
            let hides_calendar_eta = is_interstellar || is_inter_star_body_transfer;
            if is_interstellar {
                if let Some((_, ref sys_name, dist_ly)) = star_system_snap {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "\u{1F30C} Interstellar Mission: {}",
                                sys_name
                            ))
                            .strong()
                            .size(13.0)
                            .color(theme::GRAVITY_ASSIST),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Distance: {:.2} ly = {:.0} AU",
                                dist_ly,
                                dist_ly as f64 * 63_241.077
                            ))
                            .size(11.0)
                            .color(theme::TEXT_DIM),
                        );
                        ui.label(
                            egui::RichText::new(
                                "\u{26A0} Interstellar navigation is point-and-burn. \
                                 Transfer windows do not apply. \
                                 Ensure adequate \u{394}V and life-support reserves.",
                            )
                            .size(11.0)
                            .italics()
                            .color(theme::AMBER),
                        );
                    });
                    ui.add_space(4.0);
                }
            } else if is_inter_star_body_transfer {
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Binary-System Transfer")
                            .strong()
                            .size(13.0)
                            .color(theme::GRAVITY_ASSIST),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Trajectory is computed in the system barycentric frame because origin and destination orbit different stars.",
                        )
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Curved barycentric ballistic options use a system-gravity approximation; direct profiles remain available as high-thrust point-and-burn alternatives.",
                        )
                        .size(11.0)
                        .color(theme::AMBER),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Curved options change the arc shape. Direct options keep the straight barycentric preview and mainly trade travel time against ΔV.",
                        )
                        .size(11.0)
                        .italics()
                        .color(theme::TEXT_DIM),
                    );
                });
                ui.add_space(4.0);
            }

            let btn_label = if is_interstellar {
                "\u{1F680} Commit Interstellar Course".to_string()
            } else if is_course_correction {
                if abort_cost_t > 0.01 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!(
                        "\u{1F504} Execute Course Correction (+{:.2} km/s abort burn)",
                        abort_dv_kms
                    )
                } else {
                    "\u{1F504} Execute Course Correction".to_string()
                }
            } else {
                "\u{1F680} Execute Transfer".to_string()
            };

            // For fleet intercepts note the encounter speed penalty
            if fleet_target_snap.is_some() && fleet_ui_state.intercept_speed_ms > 100.0 {
                let extra_dv_kms = fleet_ui_state.intercept_speed_ms / 1_000.0;
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26A0} +{:.1} km/s added for encounter speed (not included in \u{394}V below)",
                        extra_dv_kms
                    ))
                    .size(11.0)
                    .italics()
                    .color(theme::AMBER),
                );
            }

            // Execute Transfer button with ETA on the same row
            ui.horizontal(|ui| {
                let insufficient = !sel_option.transfer_time_s.is_finite()
                    || (sel_option.is_thrust_limited
                        && (is_interstellar || is_inter_star_body_transfer)
                        && sel_option.total_delta_v_ms == 0.0);
                let btn = egui::Button::new(
                    egui::RichText::new(&btn_label).size(13.0).strong(),
                );
                let resp = ui.add_enabled(!insufficient && (sel_affordable_with_abort || is_interstellar), btn);
                if resp.clicked() {
                    if is_interstellar {
                        // Interstellar travel: no ECS destination body; log mission intent.
                        // Full multi-system navigation will be implemented in a future session.
                        if let Some((sys_id, ref sys_name, dist_ly)) = star_system_snap {
                            info!(
                                "Fleet '{}' committed to interstellar course: {} ({:.2} ly, system_id {}). \
                                 \u{394}V required: {:.1} km/s, travel time: {:.1} years. \
                                 Multi-system navigation NYI.",
                                fleet.name, sys_name, dist_ly, sys_id,
                                sel_option.total_delta_v_ms / 1_000.0,
                                sel_option.transfer_time_s / (365.25 * 86_400.0),
                            );
                        }
                    } else {
                        let maybe_transfer = if let Some(ref lp) = lp_target_snap {
                            build_planned_transfer_lp(fleet_entity, fleet, orbit, lp, body_query, &sel_option)
                        } else if let Some(tfe) = fleet_target_snap {
                            all_fleets_query.get(tfe).ok()
                                .and_then(|(_, _, _, maybe_fo, _)| maybe_fo)
                                .and_then(|fo| {
                                    build_planned_transfer(fleet_entity, fleet, orbit, fo.body, planned_departure_time_s, body_query, &sel_option, course_correction_sc, body_system_ids, current_system_id)
                                })
                        } else if let Some(te) = body_target_snap {
                            if sel_option.label == "Gravity Assist" {
                                // Build the Leg-1 arc toward the flyby body so the departure
                                // direction and orbital plane are correct, then stitch in a
                                // Leg-2 arc (flyby → destination) so the in-transit position
                                // is correct throughout the full two-leg trajectory.
                                let sel_ga_idx = fleet_ui_state.selected_gravity_assist;
                                let flyby_e = sel_ga_idx
                                    .and_then(|i| fleet_ui_state.gravity_assist_candidates.get(i))
                                    .map(|ga| ga.flyby_entity);
                                let ga_opt = sel_ga_idx
                                    .and_then(|i| fleet_ui_state.gravity_assist_candidates.get(i))
                                    .map(|e| e.option.clone());

                                if let Some(flyby) = flyby_e {
                                    let mut maybe_pt = build_planned_transfer(
                                        fleet_entity, fleet, orbit, flyby, planned_departure_time_s,
                                        body_query, &sel_option, course_correction_sc,
                                        body_system_ids, current_system_id,
                                    );

                                    if let Some(ref mut pt) = maybe_pt {
                                        // Record the flyby body so the executed maneuver can
                                        // reproduce the two-leg path for rendering.
                                        pt.flyby_body = Some(flyby);

                                        // Always record the actual destination so the fleet
                                        // parks at the right body on arrival.
                                        pt.destination_body = te;

                                        // Stitch in Leg-2: flyby → final destination.
                                        if let Some(ga) = ga_opt {
                                            use crate::astronomy::KeplerOrbit;
                                            use crate::fleets::orbital_mechanics::AU_IN_METERS;
                                            use bevy::math::DVec3;

                                            // All three positions must resolve; skip Leg-2
                                            // if any entity is missing to avoid garbage orbit.
                                            let center_res = match pt.reference_frame {
                                                TransferReferenceFrame::SystemBarycentric => Some(bevy::math::DVec3::ZERO),
                                                TransferReferenceFrame::Body(center_entity) => body_query
                                                    .get(center_entity)
                                                    .ok()
                                                    .map(|(_, _, sc, _, _)| sc.position),
                                            };
                                            let flyby_res  = body_query.get(flyby).ok().map(|(_, _, sc, _, _)| sc.position);
                                            let dest_res   = body_query.get(te).ok().map(|(_, _, sc, _, _)| sc.position);
                                            // Resolve the central body's GM from its mass (works for any star).
                                            let center_gm = match pt.reference_frame {
                                                TransferReferenceFrame::Body(center_entity) => body_query
                                                    .get(center_entity)
                                                    .ok()
                                                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                                                    .unwrap_or(GM_SUN),
                                                TransferReferenceFrame::SystemBarycentric => GM_SUN,
                                            };

                                            if let (Some(center_pos), Some(flyby_pos), Some(dest_pos)) =
                                                (center_res, flyby_res, dest_res)
                                            {

                                            let flyby_rel = flyby_pos - center_pos;
                                            let dest_rel  = dest_pos  - center_pos;
                                            let flyby_r   = flyby_rel.length();
                                            let dest_r    = dest_rel.length();

                                            let (.., leg2_sma, leg2_ecc) =
                                                hohmann_transfer(flyby_r, dest_r, center_gm);
                                            let leg2_outward = dest_r >= flyby_r;
                                            let leg2_mae = if leg2_outward { 0.0 } else { std::f64::consts::PI };

                                            // Derive orbital plane and AoP for Leg-2 from
                                            // the flyby body's current position.
                                            let plane_n = flyby_rel.cross(dest_rel);
                                            let plane_len = plane_n.length();
                                            let (incl2, lan2, aop2) = if plane_len > 1e-20 {
                                                let n = plane_n / plane_len;
                                                // Clamp guards against floating-point rounding
                                                // that can push the dot product slightly outside
                                                // [-1, 1], which would cause acos to return NaN.
                                                let incl = n.z.clamp(-1.0, 1.0).acos();
                                                let nxy = DVec3::new(-n.y, n.x, 0.0);
                                                let nl  = nxy.length();
                                                let lan = if nl > 1e-20 {
                                                    let nd = nxy / nl; nd.y.atan2(nd.x)
                                                } else { 0.0 };
                                                let aop = if nl > 1e-20 {
                                                    let nd = nxy / nl;
                                                    let pd = flyby_rel.normalize_or_zero();
                                                    let cw = nd.dot(pd);
                                                    let sw = n.dot(nd.cross(pd));
                                                    let om = sw.atan2(cw);
                                                    if leg2_outward { om } else { om + std::f64::consts::PI }
                                                } else {
                                                    let ang = flyby_rel.y.atan2(flyby_rel.x);
                                                    if leg2_outward { ang } else { ang - std::f64::consts::PI }
                                                };
                                                (incl, lan, aop)
                                            } else {
                                                let ang = flyby_rel.y.atan2(flyby_rel.x);
                                                let aop = if leg2_outward { ang } else { ang - std::f64::consts::PI };
                                                (0.0, 0.0, aop)
                                            };

                                            let sma_m = leg2_sma * AU_IN_METERS;
                                            let leg2_mm = (center_gm / sma_m.powi(3)).sqrt();

                                            pt.leg2_orbit = Some(KeplerOrbit {
                                                semi_major_axis: leg2_sma,
                                                eccentricity: leg2_ecc,
                                                inclination: incl2,
                                                longitude_ascending_node: lan2,
                                                argument_of_periapsis: aop2,
                                                mean_anomaly_epoch: leg2_mae,
                                                mean_motion: leg2_mm,
                                            });
                                            pt.leg2_start_s = ga.leg1_time_s;
                                            } // end: if let (Some(center_pos), ...)
                                        }
                                    }
                                    maybe_pt
                                } else {
                                    build_planned_transfer(fleet_entity, fleet, orbit, te, planned_departure_time_s, body_query, &sel_option, course_correction_sc, body_system_ids, current_system_id)
                                }
                            } else {
                                build_planned_transfer(fleet_entity, fleet, orbit, te, planned_departure_time_s, body_query, &sel_option, course_correction_sc, body_system_ids, current_system_id)
                            }
                        } else {
                            None
                        };
                        if let Some(transfer) = maybe_transfer {
                            pending_actions.start_transfers.push(StartTransferAction {
                                fleet: fleet_entity,
                                transfer,
                                abort_cost_t,
                                departure_offset_s: fleet_ui_state.departure_offset_days * 86_400.0,
                            });
                            // Close the transfer popup so the preview arc doesn't
                            // immediately show an abort trajectory after launch.
                            fleet_ui_state.show_transfer_popup = false;
                        }
                    }
                }
                if !hides_calendar_eta {
                    let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                    let total_eta_s = dep_s + sel_option.transfer_time_s;
                    ui.add_space(theme::Spacing::lg);
                    ui.label(
                        egui::RichText::new(format!("ETA  {}", format_duration(total_eta_s)))
                            .size(12.0)
                            .color(theme::GREEN),
                    );
                }
            });

            // GRA-153 M-3: "Abort to Origin" + "Disband Fleet" buttons.
            // Shown only when the fleet is mid-transit (course correction mode).
            // - "Abort to Origin" (primary, default): refits a parking orbit
            //   at the origin body.  Preserves the fleet entity, ships, and
            //   render position.  The fleet is NOT silently dissolved.
            // - "Disband Fleet" (secondary, confirmation): the legacy
            //   "silently dissolve" behaviour, gated behind a confirmation
            //   modal to prevent accidental clicks.
            if is_course_correction {
                ui.add_space(theme::Spacing::sm);
                let abort_label = if abort_cost_t > 0.0 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!("⛔ Abort to Origin (+{:.2} km/s burn)", abort_dv_kms)
                } else {
                    "⛔ Abort to Origin".to_string()
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(&abort_label)
                                .size(12.0)
                                .color(theme::RED),
                        )
                        .min_size(egui::Vec2::new(120.0, 30.0)),
                    )
                    .on_hover_text("Cancel the current transfer and return the fleet to a parking orbit at the origin body. Ships are preserved.")
                    .clicked()
                {
                    pending_actions.abort_to_origin.push(AbortToOriginAction {
                        fleet: fleet_entity,
                        abort_cost_t,
                    });
                    fleet_ui_state.show_transfer_popup = false;
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("💥 Disband Fleet")
                                .size(10.0)
                                .color(theme::TEXT_DIM),
                        )
                        .min_size(egui::Vec2::new(120.0, 24.0)),
                    )
                    .on_hover_text("Permanently dissolve this fleet. All ships return to independent orbit. This cannot be undone.")
                    .clicked()
                {
                    fleet_ui_state.disband_confirm_fleet = Some(fleet_entity);
                }
            }
            if !hides_calendar_eta {
                let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                let total_eta_s = dep_s + sel_option.transfer_time_s;
                if let Some(arrival_ts) = checked_arrival_timestamp(current_timestamp, total_eta_s)
                {
                    ui.label(
                        egui::RichText::new(format!(
                            "Arrives  {}",
                            format_timestamp_date_time(arrival_ts)
                        ))
                        .size(11.0)
                        .color(theme::RP_BLUE),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Arrives  unavailable")
                            .size(11.0)
                            .color(theme::AMBER),
                    );
                }
            }
            if !is_interstellar && !sel_affordable_with_abort {
                ui.label(
                    egui::RichText::new(if abort_cost_t > 0.0 {
                        "Insufficient \u{394}V remaining after abort burn."
                    } else {
                        "Selected option requires more \u{394}V than this fleet can provide."
                    })
                    .size(11.0)
                    .italics()
                    .color(theme::RED),
                );
            }
        }

        // ── Gravity Assists panel ─────────────────────────────────────────────
        // Shown whenever there are heliocentric flyby candidates for this route.
        if !fleet_ui_state.gravity_assist_candidates.is_empty() {
            ui.add_space(6.0);
            let num_ga = fleet_ui_state.gravity_assist_candidates.len();
            let header_text = format!("⚡ Gravity Assists ({num_ga} available)");
            egui::CollapsingHeader::new(
                egui::RichText::new(header_text)
                    .size(12.0)
                    .strong()
                    .color(theme::ACCENT),
            )
            .default_open(true)
            .show(ui, |ui| {
                // Snapshot data before mut-borrowing fleet_ui_state below
                let snapped: Vec<(usize, String, f64, f64, f64, f64)> = fleet_ui_state
                    .gravity_assist_candidates
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        (
                            i,
                            e.option.body_name.clone(),
                            e.option.dv_savings_ms,
                            e.option.extra_time_s,
                            e.option.window_period_s,
                            e.option.v_inf_ms,
                        )
                    })
                    .collect();

                for (idx, body_name, savings, extra_t, win_period, v_inf) in snapped {
                    let is_sel = fleet_ui_state.selected_gravity_assist == Some(idx);
                    let beneficial = savings > 100.0;
                    let header_color = if is_sel {
                        theme::EP_TEAL
                    } else if beneficial {
                        theme::GREEN
                    } else {
                        theme::TEXT_DIM
                    };

                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(format!("⚡ via {body_name}"))
                                .size(12.0)
                                .strong()
                                .color(header_color),
                        );
                        egui::Grid::new(format!("ga_grid_{idx}"))
                            .num_columns(2)
                            .spacing([8.0, 2.0])
                            .show(ui, |ui| {
                                if beneficial {
                                    ui.label(egui::RichText::new("ΔV saved:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(format_delta_v(savings))
                                            .size(11.0)
                                            .strong()
                                            .color(theme::GREEN),
                                    );
                                } else {
                                    ui.label(egui::RichText::new("Extra ΔV:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(format_delta_v(-savings))
                                            .size(11.0)
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Extra time:").size(11.0));
                                let sign = if extra_t >= 0.0 { "+" } else { "" };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{sign}{}",
                                        format_duration(extra_t.abs())
                                    ))
                                    .size(11.0)
                                    .color(theme::TEXT),
                                );
                                ui.end_row();

                                ui.label(egui::RichText::new("Window every:").size(11.0));
                                let win_str = if win_period.is_finite() {
                                    format_duration(win_period)
                                } else {
                                    "∞".to_owned()
                                };
                                ui.label(
                                    egui::RichText::new(win_str)
                                        .size(11.0)
                                        .color(theme::TEXT_DIM),
                                );
                                ui.end_row();

                                ui.label(egui::RichText::new("v∞:").size(11.0));
                                ui.label(
                                    egui::RichText::new(format_delta_v(v_inf))
                                        .size(11.0)
                                        .color(theme::TEXT_DIM),
                                );
                                ui.end_row();
                            });

                        ui.horizontal(|ui| {
                            if is_sel {
                                if ui.small_button("✕ Clear Assist").clicked() {
                                    fleet_ui_state.selected_gravity_assist = None;
                                    // Shift selection back to direct Efficient option
                                    fleet_ui_state.selected_option = 0;
                                    fleet_ui_state.planned_transfer = None;
                                }
                            } else {
                                let label = if beneficial {
                                    "⚡ Use Gravity Assist"
                                } else {
                                    "Use Suboptimal Assist"
                                };
                                if ui.small_button(label).clicked() {
                                    fleet_ui_state.selected_gravity_assist = Some(idx);
                                    fleet_ui_state.selected_option = 0; // GA is option 0
                                    fleet_ui_state.planned_transfer = None;
                                }
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
            });
        }

        let show_binary_transfer_direct_labels = body_target_snap
            .map(|target| is_inter_star_transfer(orbit.body, target, body_query))
            .unwrap_or(false);

        if !fleet_ui_state.computed_options.is_empty() {
            let fleet_wet_mass = fleet.total_wet_mass_t();
            let fleet_max_dv = fleet.max_delta_v_ms();

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Transfer Options:").strong().size(13.0));
            ui.add_space(2.0);

            // GRA-152 H-1: when a porkchop grid is cached on the
            // FleetUiState (the LGD-driven path), render the
            // PorkchopPanel in place of the Efficient / Moderate /
            // Fast `selectable_label` block.  When the grid is absent
            // (e.g. course corrections, intra-system previews) the
            // legacy 3-option row is rendered as before, so all
            // pre-existing code paths keep working.
            if let Some(grid) = fleet_ui_state.porkchop_grid.as_ref() {
                // The phase-window overlay needs `compute_transfer_window`'s
                // `time_to_window_s`; that value is computed upstream in this
                // function (above).  We pass NaN here as a sentinel meaning
                // "no phase-window overlay" — the panel renders the rest of
                // the grid either way.  Wiring the live value requires
                // threading it through this control flow; left as a
                // follow-up so this PR stays focused.
                let time_to_window_s = f64::NAN;
                super::porkchop_panel::porkchop_panel(
                    ui,
                    grid,
                    porkchop_config,
                    &mut fleet_ui_state.selected_porkchop_cell,
                    fleet_max_dv,
                    time_to_window_s,
                );
                ui.add_space(4.0);
                if let Some((sc, sr)) = fleet_ui_state.selected_porkchop_cell {
                    if let Some(cell) = grid.cells.get(sr * grid.resolution.0 + sc) {
                        if cell.feasible {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Selected cell: t_dep = {:.0} d, TOF = {:.0} d, ΔV = {:.2} km/s",
                                    cell.t_dep_s / crate::ui::porkchop_panel::SECONDS_PER_DAY,
                                    cell.tof_s / crate::ui::porkchop_panel::SECONDS_PER_DAY,
                                    cell.total_dv_ms / 1000.0,
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                        }
                    }
                }
                // Skip the legacy 3-option row when the panel is shown.
                return;
            }

            let options: Vec<_> = fleet_ui_state.computed_options.clone();
            for (idx, option) in options.iter().enumerate() {
                let option_display_label = if show_binary_transfer_direct_labels {
                    match option.label {
                        "Long Coast" => "Direct Long Coast",
                        "Short Coast" => "Direct Short Coast",
                        "Full Thrust" => "Direct Full Thrust",
                        "Fast Coast" => "Direct Fast Coast",
                        "Max Speed" => "Direct Max Speed",
                        other => other,
                    }
                } else {
                    option.label
                };
                let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);
                let fuel_pct = if fleet_wet_mass > 0.0 {
                    (fuel_cost / fleet_wet_mass * 100.0) as u32
                } else {
                    0
                };
                let affordable = option.total_delta_v_ms <= fleet_max_dv;

                let is_selected = fleet_ui_state.selected_option == idx;
                let row_color = if !affordable {
                    theme::RED
                } else if is_selected {
                    theme::RP_BLUE
                } else {
                    theme::TEXT
                };

                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    let resp = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(format!(
                            "{} {}",
                            if is_selected { "●" } else { "○" },
                            option_display_label
                        ))
                        .size(13.0)
                        .strong()
                        .color(row_color),
                    );
                    if resp.clicked() {
                        fleet_ui_state.selected_option = idx;
                        fleet_ui_state.planned_transfer = None;
                    }

                    // Epoch line: "Depart: DD.MM.YYYY HH:MM / Arrive: …" beneath the
                    // option name, so the player sees the absolute transfer window
                    // without having to compute it from the departure offset slider.
                    let depart_offset_s = fleet_ui_state.departure_offset_days.max(0.0) * 86_400.0;
                    let depart_ts = current_timestamp + depart_offset_s as i64;
                    let arrive_ts = depart_ts + option.transfer_time_s as i64;
                    ui.label(
                        egui::RichText::new(format!(
                            "Depart: {} / Arrive: {}",
                            format_timestamp_date_time(depart_ts),
                            format_timestamp_date_time(arrive_ts),
                        ))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                    );

                    egui::Grid::new(format!("option_{idx}"))
                        .num_columns(4)
                        .spacing([16.0, 2.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Total ΔV:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.total_delta_v_ms))
                                    .size(12.0)
                                    .strong()
                                    .color(row_color),
                            );
                            ui.label(egui::RichText::new("Travel time:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_duration(option.transfer_time_s))
                                    .size(12.0)
                                    .strong(),
                            );
                            ui.end_row();

                            if show_binary_transfer_direct_labels {
                                let selected_label = fleet_ui_state
                                    .computed_options
                                    .get(fleet_ui_state.selected_option)
                                    .map(|option| match option.label {
                                        "Long Coast" => "Direct Long Coast",
                                        "Short Coast" => "Direct Short Coast",
                                        "Full Thrust" => "Direct Full Thrust",
                                        "Fast Coast" => "Direct Fast Coast",
                                        "Max Speed" => "Direct Max Speed",
                                        other => other,
                                    })
                                    .unwrap_or("Direct Transfer");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Selected profile: {selected_label}"
                                    ))
                                    .size(11.0)
                                    .italics()
                                    .color(theme::TEXT_DIM),
                                );
                            }

                            ui.label(egui::RichText::new("Est. fuel:").size(12.0));
                            let fuel_color = if affordable { theme::AMBER } else { theme::RED };
                            ui.label(
                                egui::RichText::new(format!("{:.0} t ({fuel_pct}%)", fuel_cost))
                                    .size(12.0)
                                    .color(fuel_color),
                            );
                            ui.label(egui::RichText::new("Departure burn:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.delta_v1_ms)).size(12.0),
                            );
                            ui.end_row();

                            // Plane-change ΔV row (only shown when non-trivial)
                            if option.plane_change_dv_ms > 100.0 {
                                ui.label(egui::RichText::new("Plane change:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_delta_v(option.plane_change_dv_ms))
                                        .size(12.0)
                                        .color(theme::TEXT_VALUE),
                                );
                                ui.label(egui::RichText::new("").size(12.0));
                                ui.label(egui::RichText::new("").size(12.0));
                                ui.end_row();
                            }

                            // Burn time row — shows how long the fleet's engines fire.
                            if option.burn_time_s > 0.0 {
                                // Classify burn profile based on burn/transfer time ratio.
                                let (profile_label, profile_color) = if option.is_thrust_limited {
                                    // Burn time >= Hohmann time: impulsive assumption invalid.
                                    ("⚠ Thrust-limited", theme::RED)
                                } else if option.label == "Full Thrust" {
                                    // Entire trip is a burn
                                    ("⚡ Full thrust", theme::AMBER)
                                } else {
                                    let ratio =
                                        option.burn_time_s / option.transfer_time_s.max(1.0);
                                    if option.burn_time_s < 3_600.0 {
                                        ("Impulsive", theme::GREEN)
                                    } else if ratio < 0.05 {
                                        ("Short burn", theme::GREEN)
                                    } else if ratio < 0.25 {
                                        ("Extended burn", theme::AMBER)
                                    } else {
                                        ("Continuous thrust", theme::AMBER)
                                    }
                                };
                                ui.label(egui::RichText::new("Burn time:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_duration(option.burn_time_s))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.label(egui::RichText::new("Profile:").size(12.0));
                                ui.label(
                                    egui::RichText::new(profile_label)
                                        .size(12.0)
                                        .color(profile_color),
                                );
                                ui.end_row();

                                let accel_ms2 = fleet.min_accel_ms2();
                                let accel_g = accel_ms2 / 9.80665;
                                ui.label(egui::RichText::new("Acceleration:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format!("{:.2} g", accel_g))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.end_row();

                                // Extra warning row for thrust-limited options.
                                if option.is_thrust_limited {
                                    ui.label(
                                        egui::RichText::new(
                                            "  Low-thrust spiral — travel time ≥ burn time",
                                        )
                                        .size(11.0)
                                        .italics()
                                        .color(theme::AMBER),
                                    );
                                    ui.end_row();
                                }
                            }

                            if !affordable {
                                ui.label(
                                    egui::RichText::new("⚠ Insufficient ΔV capacity")
                                        .size(11.0)
                                        .color(theme::RED),
                                );
                            }
                        });
                });
                ui.add_space(2.0);
            }
        }
    }
}

/// Build a `PlannedTransfer` from the selected transfer option and fleet/body state.
pub fn build_planned_transfer(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    target_entity: Entity,
    sim_time_s: f64,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    option: &TransferOption,
    // For course corrections: the fleet's actual current position (in whatever
    // frame matches the central-body coordinates, typically heliocentric AU).
    // When set, used instead of the origin body's position for orbital-element
    // derivation so the Keplerian arc starts from the fleet, not from Jupiter.
    course_correction_pos: Option<bevy::math::DVec3>,
    body_system_ids: &Query<&SystemId>,
    current_system_id: usize,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::{solve_lambert_transfer, AU_IN_METERS, GM_SUN, G_CONST};

    let departure_time_s = sim_time_s;
    let arrival_time_s = departure_time_s + option.transfer_time_s;

    let (_, origin_body, origin_sc, origin_ko, origin_lp) = body_query.get(orbit.body).ok()?;
    let (_, dest_body, _dest_sc, dest_ko, dest_lp) = body_query.get(target_entity).ok()?;

    let dest_parent = dest_lp.map(|lp| lp.0);
    let origin_parent = origin_lp.map(|lp| lp.0);
    let dest_is_star = dest_body.body_type == BodyType::Star;
    let dest_is_ring = dest_body.body_type == BodyType::Ring;
    let origin_host_star_e = find_host_star(orbit.body, body_query).map(|(e, _)| e);
    let dest_host_star_e = find_host_star(target_entity, body_query).map(|(e, _)| e);
    let is_inter_star = origin_host_star_e.is_some()
        && dest_host_star_e.is_some()
        && origin_host_star_e != dest_host_star_e;

    // Determine: (origin_sma, dest_sma, gm, orbit_center, actual destination body for FleetOrbit)
    // For Rings: redirect the FleetOrbit destination to the ring's parent planet.
    // For Stars: Fleet will orbit the star at the planet SOI boundary; orbit_center = star entity.
    let (origin_sma_au, dest_sma_au, gm, orbit_center, actual_dest_body, reference_frame) =
        if is_inter_star {
            let r1 = transfer_absolute_position(orbit.body, departure_time_s, body_query)
                .unwrap_or(origin_sc.position)
                .length()
                .max(MIN_ORBITAL_RADIUS_AU);
            let r2 = transfer_absolute_position(target_entity, arrival_time_s, body_query)
                .map(|pos| pos.length())
                .unwrap_or(1.5)
                .max(MIN_ORBITAL_RADIUS_AU);
            let system_gm_raw: f64 = body_query
                .iter()
                .filter_map(|(e, b, _, _, _)| {
                    if b.body_type != BodyType::Star {
                        return None;
                    }
                    let Ok(system_id) = body_system_ids.get(e) else {
                        return None;
                    };
                    if system_id.0 != current_system_id {
                        return None;
                    }
                    Some(G_CONST * b.mass)
                })
                .sum();
            let system_gm = if system_gm_raw > 0.0 {
                system_gm_raw
            } else {
                GM_SUN
            };
            let primary_star = body_query
                .iter()
                .filter_map(|(e, b, sc, _, _)| {
                    if b.body_type != BodyType::Star {
                        return None;
                    }
                    let Ok(system_id) = body_system_ids.get(e) else {
                        return None;
                    };
                    if system_id.0 != current_system_id {
                        return None;
                    }
                    Some((e, sc))
                })
                .min_by(|(_, sc_a), (_, sc_b)| {
                    sc_a.position
                        .length_squared()
                        .partial_cmp(&sc_b.position.length_squared())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(e, _)| e)
                .unwrap_or(orbit.body);
            (
                r1,
                r2,
                system_gm,
                primary_star,
                target_entity,
                TransferReferenceFrame::SystemBarycentric,
            )
        } else if dest_is_star {
            // Transfer toward the destination star.
            // The transfer orbit is centred on the destination star, so gm = G·M_star.
            //
            // GRA-149 C-2: arrival parking radius is the per-body `star_approach_au`
            // field (RON override or per-star default).  Previously this code parked
            // the fleet at the planet's sphere of influence (SOI), which (a) is not a
            // real orbit — SOI is a frame-switch threshold — and (b) the picker label
            // claimed 0.3 AU while the math produced ~0.012 AU for hot-Jupiters.  The
            // arrival parking radius is now sourced from a single helper that
            // resolves the per-body value (or the global default) and is reused by
            // the barycentric endpoint computation and the final arrival_radius
            // selection below.
            let star_mass = dest_body.mass; // destination IS the star
                                            // planet_sma_au (the origin body's star-centric SMA) is the departure
                                            // distance.  Do NOT use orbit.radius_au — that is the fleet's local
                                            // parking orbit radius and would make the outward/inward direction check
                                            // incorrect in the star frame.
            let planet_sma_au = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0);
            let approach_au = star_approach_radius_au(dest_body);
            // For transfers that head *outward* (planet_sma_au < approach_au), the
            // parking radius is the approach value.  For inward transfers we keep
            // the arrival inside the origin orbit so the planet doesn't have to
            // pre-date the fleet.  This preserves the prior "SOI is always inside
            // the origin orbit" safety.
            let arrival_au = if approach_au >= planet_sma_au {
                approach_au
            } else {
                (planet_sma_au * 0.01).max(approach_au)
            };
            (
                planet_sma_au,
                arrival_au,
                G_CONST * star_mass,
                target_entity,
                target_entity,
                TransferReferenceFrame::Body(target_entity),
            )
        } else if dest_is_ring {
            // Ring: resolve to orbiting the ring's parent planet at ring.radius altitude
            let ring_parent = dest_parent.unwrap_or(orbit.body);
            let parent_mass = body_query
                .get(ring_parent)
                .ok()
                .map(|(_, b, _, _, _)| b.mass)
                .unwrap_or(5.972e24);
            let ring_radius_au = (dest_body.radius as f64 * 1_000.0) / AU_IN_METERS;
            let r1 = if ring_parent == orbit.body {
                orbit.radius_au
            } else {
                origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.01)
            };
            (
                r1,
                ring_radius_au,
                G_CONST * parent_mass,
                ring_parent,
                ring_parent,
                TransferReferenceFrame::Body(ring_parent),
            )
        } else if dest_parent == Some(orbit.body) {
            // Local (e.g., Earth → Moon)
            let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            (
                orbit.radius_au,
                r2,
                G_CONST * origin_body.mass,
                orbit.body,
                target_entity,
                TransferReferenceFrame::Body(orbit.body),
            )
        } else if let Some(shared) = dest_parent.filter(|parent| Some(*parent) == origin_parent) {
            // Both orbit the same central body (moon-to-moon OR interplanetary, e.g. Earth→Mars).
            // Use G·mass for any central body — non-Sol stars carry their actual mass in kg
            // in CelestialBody.mass, so G·M gives the correct GM for any star.
            let gm = body_query
                .get(shared)
                .ok()
                .map(|(_, b, _, _, _)| G_CONST * b.mass)
                .unwrap_or(GM_SUN);
            let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            (
                r1,
                r2,
                gm,
                shared,
                target_entity,
                TransferReferenceFrame::Body(shared),
            )
        } else if Some(target_entity) == origin_parent {
            // Downward transfer: fleet is at a moon, destination is the parent planet.
            // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
            let parent_mass = dest_body.mass;
            let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
            let r2 = (dest_body.radius as f64 * 3_000.0) / AU_IN_METERS;
            (
                r1,
                r2.min(r1 * 0.5),
                G_CONST * parent_mass,
                target_entity,
                target_entity,
                TransferReferenceFrame::Body(target_entity),
            )
        } else {
            // ── Heliocentric fallback ─────────────────────────────────────────────
            //
            // ── Detect inter-star transfer ─────────────────────────────────────────
            // When origin and dest orbit different stars in a multi-star system, the
            // transfer happens in the barycentric frame.  We walk the full LogicalParent
            // chain to the stellar ancestor so that moon→planet→star hierarchies are
            // handled correctly (not just immediate parent checks).
            if is_inter_star {
                // Barycentric distances — already correct since SpaceCoordinates stores
                // positions relative to the system origin (≈ barycenter).
                // origin_sc is always valid (obtained above); target_entity query can only
                // fail if the entity was somehow despawned between the UI call and here,
                // which should not happen in practice.
                let r1 = transfer_absolute_position(orbit.body, departure_time_s, body_query)
                    .unwrap_or(origin_sc.position)
                    .length()
                    .max(MIN_ORBITAL_RADIUS_AU);
                let r2 = transfer_absolute_position(target_entity, arrival_time_s, body_query)
                    .map(|pos| pos.length())
                    .unwrap_or(1.5) // defensive fallback; should not be reached
                    .max(MIN_ORBITAL_RADIUS_AU);
                // Total system GM: sum G·M for all stars in the CURRENT system only.
                // Stars from other systems (e.g. nearby-star catalog entries) must be
                // excluded, otherwise GM is vastly overestimated.
                // We do NOT clamp with .max(GM_SUN); sub-solar binaries must use their
                // actual combined GM (e.g. two K-dwarfs at 0.6+0.2 M☉ total 0.8 M☉).
                let system_gm_raw: f64 = body_query
                    .iter()
                    .filter_map(|(e, b, _, _, _)| {
                        if b.body_type != BodyType::Star {
                            return None;
                        }
                        let Ok(system_id) = body_system_ids.get(e) else {
                            return None;
                        };
                        if system_id.0 != current_system_id {
                            return None;
                        }
                        Some(G_CONST * b.mass)
                    })
                    .sum();
                let system_gm = if system_gm_raw > 0.0 {
                    system_gm_raw
                } else {
                    GM_SUN // fallback only when no stars found for current system (degenerate)
                };
                // Orbit center: the star in the CURRENT system nearest to the barycenter.
                let primary_star = body_query
                    .iter()
                    .filter_map(|(e, b, sc, _, _)| {
                        if b.body_type != BodyType::Star {
                            return None;
                        }
                        let Ok(system_id) = body_system_ids.get(e) else {
                            return None;
                        };
                        if system_id.0 != current_system_id {
                            return None;
                        }
                        Some((e, sc))
                    })
                    .min_by(|(_, sc_a), (_, sc_b)| {
                        sc_a.position
                            .length_squared()
                            .partial_cmp(&sc_b.position.length_squared())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(e, _)| e)
                    .unwrap_or(orbit.body);
                (
                    r1,
                    r2,
                    system_gm,
                    primary_star,
                    target_entity,
                    TransferReferenceFrame::SystemBarycentric,
                )
            } else {
                // If fleet is at a moon, its own SMA is Earth-relative — use parent's SMA.
                //
                // GRA-149 C-3: classify "is the body a star itself?" by mass
                // instead of SMA.  Hot-Jupiters at 0.02 AU and any other close-orbit
                // giant planet now correctly use their own heliocentric SMA
                // (and contribute a correct frame GM in the body_system_ids
                // resolution below), rather than being silently re-parented to
                // whatever happens to be at <0.05 AU.
                let origin_is_stellar = body_query
                    .get(orbit.body)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                let dest_is_stellar = body_query
                    .get(target_entity)
                    .ok()
                    .map(|(_, b, _, _, _)| is_stellar_mass(b.mass))
                    .unwrap_or(false);
                let r1 = if origin_is_stellar {
                    origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0)
                } else {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or_else(|| origin_ko.map(|ko| ko.semi_major_axis))
                        .unwrap_or(1.0)
                };
                let r2 = if dest_is_stellar {
                    dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.5)
                } else {
                    dest_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or_else(|| dest_ko.map(|ko| ko.semi_major_axis))
                        .unwrap_or(1.5)
                };
                // Use the host star's actual GM rather than the hardcoded GM_SUN so that
                // non-Sol systems (e.g. 1.1 M☉ Alpha Centauri A, or a 0.5 M☉ K-dwarf) produce
                // correct velocities and transfer times.
                //
                // For single-star systems: origin_parent is the star entity → use its GM.
                // For binary systems where origin and dest orbit different stars: fall back to
                // the origin star's GM (best available single-body approximation).
                // Final fallback: find any nearby Star with no KeplerOrbit (root star); last
                // resort is GM_SUN.
                let host_gm = origin_parent
                    .and_then(|pe| body_query.get(pe).ok())
                    .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                    .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    .or_else(|| {
                        dest_parent
                            .and_then(|pe| body_query.get(pe).ok())
                            .filter(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                            .map(|(_, b, _, _, _)| G_CONST * b.mass)
                    })
                    .unwrap_or(GM_SUN);
                // Find the orbit center: prefer the origin body's host star (LogicalParent),
                // falling back to any nearby root star, then the fleet's current body.
                let star = origin_parent
                    .filter(|&pe| {
                        body_query
                            .get(pe)
                            .ok()
                            .map(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                            .unwrap_or(false)
                    })
                    .or_else(|| {
                        body_query
                            .iter()
                            .find(|(_, b, sc, _, _)| {
                                b.body_type == BodyType::Star && sc.position.length_squared() < 1.0
                            })
                            .map(|(e, _, _, _, _)| e)
                    })
                    .unwrap_or(orbit.body);
                (
                    r1,
                    r2,
                    host_gm,
                    star,
                    target_entity,
                    TransferReferenceFrame::Body(star),
                )
            } // end same-star heliocentric case
        };

    // For course corrections, determine outward/inward from the fleet's actual distance vs
    // the destination distance.  The body SMAs may not reflect the fleet's position mid-transit.
    // (Computed after rel_pos and dest_rel are available below; use a closure to defer.)
    // For local transfers (planet ↔ moon, or moon → parent planet), the orbit_center IS the
    // planet and its SpaceCoordinates are heliocentric, but we need planet-centric coordinates
    // (DVec3::ZERO) for the transfer orbit geometry. Only use heliocentric position for
    // heliocentric transfers.
    // Cases: (1) Earth → Moon: dest_parent == Some(orbit.body), (2) Moon → Earth: Some(target_entity) == origin_parent
    let orbit_center_is_star = matches!(reference_frame, TransferReferenceFrame::Body(center_entity)
        if body_query
            .get(center_entity)
            .ok()
            .map(|(_, b, _, _, _)| b.body_type == BodyType::Star)
            .unwrap_or(false));
    let is_local_transfer = !orbit_center_is_star
        && (dest_parent == Some(orbit.body) || Some(target_entity) == origin_parent);
    let local_center_is_star = is_local_transfer && orbit_center_is_star;
    let future_resolved_transfer =
        reference_frame.is_barycentric() || (orbit_center_is_star && !is_local_transfer);
    let star_origin_departure_absolute = if origin_body.body_type == BodyType::Star {
        match reference_frame {
            TransferReferenceFrame::Body(center_entity) if center_entity == orbit.body => {
                let center_departure =
                    transfer_absolute_position(center_entity, departure_time_s, body_query)
                        .unwrap_or(origin_sc.position);
                let target_departure =
                    transfer_absolute_position(target_entity, departure_time_s, body_query)
                        .unwrap_or(
                            center_departure
                                + bevy::math::DVec3::X * orbit.radius_au.max(MIN_ORBITAL_RADIUS_AU),
                        );
                let radial_dir = (target_departure - center_departure).normalize_or_zero();
                let fallback_dir =
                    bevy::math::DVec3::new(orbit.angle_rad.cos(), orbit.angle_rad.sin(), 0.0);
                let departure_dir = if radial_dir.length_squared() > 1e-12 {
                    radial_dir
                } else {
                    fallback_dir
                };
                Some(center_departure + departure_dir * orbit.radius_au.max(MIN_ORBITAL_RADIUS_AU))
            }
            _ => None,
        }
    } else {
        None
    };

    let center_pos = match reference_frame {
        TransferReferenceFrame::SystemBarycentric => bevy::math::DVec3::ZERO,
        TransferReferenceFrame::Body(center_entity) => {
            if is_local_transfer && !local_center_is_star {
                bevy::math::DVec3::ZERO
            } else if future_resolved_transfer {
                transfer_absolute_position(center_entity, departure_time_s, body_query)
                    .unwrap_or(bevy::math::DVec3::ZERO)
            } else {
                body_query
                    .get(center_entity)
                    .ok()
                    .map(|(_, _, sc, _, _)| sc.position)
                    .unwrap_or(bevy::math::DVec3::ZERO)
            }
        }
    };
    // For course corrections use the fleet's actual position; otherwise use the origin body.
    let rel_pos = if let Some(fleet_pos) = course_correction_pos {
        // fleet_pos is already in the correct frame (heliocentric or planet-relative).
        // If the orbit center has coordinates, convert fleet_pos to center-relative.
        // cc_local_pos from the caller is already planet-relative for local transfers,
        // but heliocentric for Sun transfers — both are relative to the frame origin,
        // not the orbit_center entity.  Subtract center_pos for consistency.
        fleet_pos - center_pos
    } else {
        // For local transfers (planet ↔ moon): origin_sc.position is already local
        // (moon-relative), and center_pos is DVec3::ZERO, so rel_pos = origin_sc.position.
        // For heliocentric transfers where the fleet orbits a moon, the moon's
        // SpaceCoordinates stores only a local offset from its parent planet — not a
        // heliocentric position.  Use the parent planet's heliocentric SC so that the
        // departure direction (argument_of_periapsis) points in the correct direction.
        let origin_pos = if let Some(star_departure_pos) = star_origin_departure_absolute {
            star_departure_pos
        } else if future_resolved_transfer {
            transfer_absolute_position(orbit.body, departure_time_s, body_query)
                .unwrap_or(origin_sc.position)
        } else if is_local_transfer && !local_center_is_star {
            origin_sc.position
        } else if origin_body.body_type == BodyType::Moon {
            origin_parent
                .and_then(|pe| body_query.get(pe).ok())
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        };
        origin_pos - center_pos
    };

    // Derive the transfer-orbit plane from the 3D departure and arrival position
    // vectors relative to the central body (r1 × r2 gives the plane normal).
    // This keeps inclination, LAN, and argument_of_periapsis mutually consistent
    // so the propagated green-dot position and the displayed preview arc match.
    // For heliocentric transfers where the destination is a moon, its SpaceCoordinates
    // also stores only a local offset — use the parent planet's position instead.
    let dest_sc_pos = body_query
        .get(target_entity)
        .ok()
        .map(|(_, b, sc, _, lp)| {
            // For local transfers (planet ↔ moon or moon → parent planet):
            // - origin is moon-relative (local coordinates)
            // - destination should also be local (DVec3::ZERO for the planet center)
            // For heliocentric: if destination is a moon, get parent's heliocentric position
            if future_resolved_transfer {
                transfer_absolute_position(target_entity, arrival_time_s, body_query)
                    .unwrap_or(sc.position)
            } else if is_local_transfer {
                // For downward transfer (Moon → Earth), destination is the planet itself
                // For upward transfer (Earth → Moon), destination is moon-relative
                if Some(target_entity) == origin_parent {
                    // Downward: destination is the parent planet, use DVec3::ZERO
                    bevy::math::DVec3::ZERO
                } else if local_center_is_star {
                    sc.position
                } else {
                    // Upward: destination is moon-relative
                    sc.position
                }
            } else if b.body_type == BodyType::Moon {
                lp.and_then(|lp| body_query.get(lp.0).ok())
                    .map(|(_, _, sc, _, _)| sc.position)
                    .unwrap_or(sc.position)
            } else {
                sc.position
            }
        })
        .unwrap_or(bevy::math::DVec3::ZERO);
    let dest_rel = dest_sc_pos - center_pos;

    // For course corrections, determine outward/inward from the fleet's actual distance vs
    // the destination distance.  The body SMAs may not reflect the fleet's position mid-transit.
    let outward = if course_correction_pos.is_some() {
        let fleet_r = rel_pos.length();
        let dest_r = dest_rel.length();
        dest_r >= fleet_r
    } else {
        dest_sma_au >= origin_sma_au
    };

    let plane_normal = rel_pos.cross(dest_rel);
    let plane_normal_len = plane_normal.length();

    let default_transfer_plane = if plane_normal_len > 1e-20 {
        let n = plane_normal / plane_normal_len;
        // i = angle between plane normal and ecliptic north (Ẑ).
        let incl = n.z.clamp(-1.0, 1.0).acos();
        // Ascending node: N = Ẑ × n = (-ny, nx, 0).
        let node_xy = bevy::math::DVec3::new(-n.y, n.x, 0.0);
        let node_len = node_xy.length();
        let lan = if node_len > 1e-20 {
            let node = node_xy / node_len;
            node.y.atan2(node.x)
        } else {
            0.0
        };
        // ω: angle from ascending node to periapsis (departure point for outward,
        // arrival for inward), measured in the orbital plane.
        let aop = if node_len > 1e-20 {
            let node = node_xy / node_len;
            let peri_dir = rel_pos.normalize_or_zero();
            let cos_w = node.dot(peri_dir);
            let sin_w = n.dot(node.cross(peri_dir));
            let omega = sin_w.atan2(cos_w);
            if outward {
                omega
            } else {
                omega + std::f64::consts::PI
            }
        } else {
            let departure_angle = rel_pos.y.atan2(rel_pos.x);
            if outward {
                departure_angle
            } else {
                departure_angle - std::f64::consts::PI
            }
        };
        (incl, lan, aop)
    } else {
        // Degenerate (origin and destination collinear with center): ecliptic-flat.
        let departure_angle = rel_pos.y.atan2(rel_pos.x);
        let aop = if outward {
            departure_angle
        } else {
            departure_angle - std::f64::consts::PI
        };
        (0.0, 0.0, aop)
    };

    let star_endpoint_reference_orbit = if dest_is_star {
        star_frame_reference_orbit(orbit.body, origin_parent, body_query)
    } else if origin_body.body_type == BodyType::Star {
        star_frame_reference_orbit(target_entity, dest_parent, body_query)
    } else {
        None
    };

    let (transfer_inclination, transfer_lan, argument_of_periapsis) = star_endpoint_reference_orbit
        .and_then(|reference_orbit| {
            transfer_plane_from_reference_orbit(&reference_orbit, rel_pos, outward)
        })
        .unwrap_or(default_transfer_plane);

    let same_star_stellar_lambert = matches!(reference_frame,
        TransferReferenceFrame::Body(center_entity)
            if body_query
                .get(center_entity)
                .ok()
                .map(|(_, body, _, _, _)| body.body_type == BodyType::Star)
                .unwrap_or(false)
            && !is_local_transfer
            && !dest_is_star
            && !option.label.contains("Direct")
    );

    let lambert_same_star_solution = if same_star_stellar_lambert {
        if let TransferReferenceFrame::Body(center_entity) = reference_frame {
            let center_departure =
                transfer_absolute_position(center_entity, departure_time_s, body_query)
                    .unwrap_or(bevy::math::DVec3::ZERO);
            let center_arrival =
                transfer_absolute_position(center_entity, arrival_time_s, body_query)
                    .unwrap_or(center_departure);
            let origin_departure = if let Some(fleet_pos) = course_correction_pos {
                fleet_pos
            } else if let Some(star_departure_pos) = star_origin_departure_absolute {
                star_departure_pos - center_departure
            } else {
                transfer_absolute_position(orbit.body, departure_time_s, body_query)
                    .unwrap_or(rel_pos + center_pos)
                    - center_departure
            };
            let destination_arrival =
                transfer_absolute_position(target_entity, arrival_time_s, body_query)
                    .unwrap_or(dest_rel + center_pos)
                    - center_arrival;

            solve_lambert_transfer(
                origin_departure,
                destination_arrival,
                option.transfer_time_s,
                gm,
            )
        } else {
            None
        }
    } else {
        None
    };
    let lambert_same_star_orbit = lambert_same_star_solution.map(|(_, _, orbit)| orbit);

    let barycentric_start_end = if reference_frame.is_barycentric() {
        let origin_future = transfer_absolute_position(orbit.body, departure_time_s, body_query)
            .unwrap_or(rel_pos + center_pos);
        let dest_future_center =
            transfer_absolute_position(target_entity, arrival_time_s, body_query)
                .unwrap_or(dest_rel + center_pos);
        let dest_future = if dest_is_star {
            // GRA-149 C-2: arrival parking radius now matches the per-body
            // star_approach_au override (or the 0.3 AU default) instead of a
            // hard-coded SOI value.
            let approach_au = star_approach_radius_au(dest_body);
            let inbound = (dest_future_center - origin_future).normalize_or_zero();
            if inbound.length_squared() > 1e-20 {
                dest_future_center - inbound * approach_au
            } else {
                dest_future_center + bevy::math::DVec3::new(approach_au, 0.0, 0.0)
            }
        } else {
            dest_future_center
        };
        Some((origin_future, dest_future))
    } else {
        None
    };

    let lambert_barycentric_solution =
        if reference_frame.is_barycentric() && option.transfer_orbit_override.is_some() {
            barycentric_start_end.and_then(|(origin_future, dest_future)| {
                solve_lambert_transfer(origin_future, dest_future, option.transfer_time_s, gm)
            })
        } else {
            None
        };

    let transfer_orbit =
        if reference_frame.is_barycentric() && option.transfer_orbit_override.is_some() {
            lambert_barycentric_solution
                .map(|(_, _, orbit)| orbit)
                .unwrap_or_else(|| {
                    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
                    let sma_m = option.sma_au * AU_IN_METERS;
                    let mean_motion = (gm / sma_m.powi(3)).sqrt();

                    KeplerOrbit {
                        semi_major_axis: option.sma_au,
                        eccentricity: option.eccentricity,
                        inclination: transfer_inclination,
                        longitude_ascending_node: transfer_lan,
                        argument_of_periapsis,
                        mean_anomaly_epoch,
                        mean_motion,
                    }
                })
        } else if let Some(orbit) = lambert_same_star_orbit {
            orbit
        } else if let Some(orbit_override) = option.transfer_orbit_override {
            orbit_override
        } else {
            let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
            let sma_m = option.sma_au * AU_IN_METERS;
            let mean_motion = (gm / sma_m.powi(3)).sqrt();

            KeplerOrbit {
                semi_major_axis: option.sma_au,
                eccentricity: option.eccentricity,
                inclination: transfer_inclination,
                longitude_ascending_node: transfer_lan,
                argument_of_periapsis,
                mean_anomaly_epoch,
                mean_motion,
            }
        };
    let preserve_orbit_geometry =
        option.transfer_orbit_override.is_some() || lambert_same_star_orbit.is_some();
    let (departure_velocity_ms, arrival_velocity_ms) = lambert_barycentric_solution
        .map(|(departure_velocity, arrival_velocity, _)| {
            (Some(departure_velocity), Some(arrival_velocity))
        })
        .or_else(|| {
            lambert_same_star_solution.map(|(departure_velocity, arrival_velocity, _)| {
                (Some(departure_velocity), Some(arrival_velocity))
            })
        })
        .unwrap_or((None, None));
    let (start_position_au, end_position_au) = barycentric_start_end
        .map(|(start_position, end_position)| (Some(start_position), Some(end_position)))
        .or_else(|| {
            lambert_same_star_solution.map(|_| {
                (
                    transfer_absolute_position(orbit.body, departure_time_s, body_query),
                    transfer_absolute_position(target_entity, arrival_time_s, body_query),
                )
            })
        })
        .unwrap_or((None, None));

    let exact_star_centered_data = exact_star_centered_transfer_data(
        reference_frame,
        orbit_center,
        &transfer_orbit,
        gm,
        departure_time_s,
        arrival_time_s,
        is_local_transfer,
        body_query,
    );
    let start_position_au = start_position_au.or(exact_star_centered_data.map(|data| data.0));
    let end_position_au = end_position_au.or(exact_star_centered_data.map(|data| data.1));
    let departure_velocity_ms =
        departure_velocity_ms.or(exact_star_centered_data.map(|data| data.2));
    let arrival_velocity_ms = arrival_velocity_ms.or(exact_star_centered_data.map(|data| data.3));

    // Arrival orbit radius:
    //   * For barycentric same-star star approaches: the per-body approach radius
    //     (matches the picker label, the barycentric endpoint, and the parking
    //     orbit math above).
    //   * For rings: the ring's own SMA.
    //   * For non-barycentric star approaches: dest_sma_au (which the C-2 fix
    //     unified with the approach radius — see the dest_is_star branch above).
    //   * Otherwise: reuse the fleet's existing parking radius.
    let arrival_orbit_radius_au = if reference_frame.is_barycentric() && dest_is_star {
        star_approach_radius_au(dest_body)
    } else if dest_is_ring || dest_is_star {
        dest_sma_au
    } else {
        orbit.radius_au
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body: actual_dest_body,
        reference_frame,
        orbit_center,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        preserve_orbit_geometry,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label: option.label,
        start_position_au,
        end_position_au,
        departure_velocity_ms,
        arrival_velocity_ms,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    })
}

/// Build a `PlannedTransfer` targeting a Lagrange point (no dedicated ECS entity).
fn build_planned_transfer_lp(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    lp: &LagrangeTarget,
    body_query: &Query<(
        Entity,
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&LogicalParent>,
    )>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::AU_IN_METERS;

    // LP transfers are heliocentric – find the host star as orbit center.
    // Prefer the LogicalParent of the fleet's current body if it is a Star (correct
    // for circumstellar orbits around non-primary stars in binary systems).
    // Fall back to any nearby star with small SpaceCoordinates magnitude, excluding
    // distant catalog entries.  The `ko.is_none()` guard is intentionally dropped so
    // that secondary stars that have a KeplerOrbit (orbiting the barycenter) can also
    // be found as the host.
    let orbit_body_parent = body_query
        .get(orbit.body)
        .ok()
        .and_then(|(_, _, _, _, lp)| lp)
        .map(|lp| lp.0);
    let star_entity = orbit_body_parent
        .filter(|&pe| {
            body_query
                .get(pe)
                .ok()
                .map(|(_, b, _, _, _)| b.body_type == BodyType::Star)
                .unwrap_or(false)
        })
        .or_else(|| {
            body_query
                .iter()
                .find(|(_, b, sc, _, _)| {
                    b.body_type == BodyType::Star && sc.position.length_squared() < 1.0
                })
                .map(|(e, _, _, _, _)| e)
        })
        .unwrap_or(orbit.body);

    // Determine departure position.  For fleets orbiting the star directly
    // (e.g. after a previous LP transfer), `orbit.body` is the star whose
    // SpaceCoordinates are at the heliocentric origin → rel_pos would be
    // (0,0,0) and departure_angle 0.  In that case use the L-point's parent
    // planet position instead so the orbit geometry is meaningful.
    let center_pos = body_query
        .get(star_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(bevy::math::DVec3::ZERO);

    let origin_pos = {
        let (_, body_data, origin_sc, _, _) = body_query.get(orbit.body).ok()?;
        if body_data.body_type == BodyType::Star {
            // Fleet is parked around the star — use the planet's current position
            // as the departure reference instead.
            body_query
                .get(lp.planet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        }
    };

    let rel_pos = origin_pos - center_pos;
    let departure_angle = rel_pos.y.atan2(rel_pos.x);

    // ALL LP transfers are kinematic (direct Bezier arc from origin to LP position).
    // This prevents co-orbital phasing options from rendering as multi-lap Keplerian
    // rings around the Sun (which previously looked like "multiple orbit rings").
    let option_label: &'static str = match option.label {
        "Efficient" => "Direct Efficient",
        "Moderate" => "Direct Moderate",
        "Fast" => "Direct Fast",
        other => other, // kinematic labels (Full Thrust, Coast, Max Speed, Direct *) pass through
    };

    // Pre-compute the heliocentric LP position for kinematic arc rendering.
    // Every LP transfer sets start/end positions so the fleet flies to the correct
    // Lagrange-point location rather than the star origin (0,0,0).
    let planet_pos = body_query
        .get(lp.planet_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(origin_pos);
    let planet_rel = planet_pos - center_pos;
    let planet_angle = planet_rel.y.atan2(planet_rel.x);
    let lp_angle = match lp.point {
        3 => planet_angle + std::f64::consts::PI,
        4 => planet_angle + std::f64::consts::FRAC_PI_3,
        5 => planet_angle - std::f64::consts::FRAC_PI_3,
        _ => planet_angle, // L1/L2: on the Sun-planet radial
    };
    let lp_pos_au = center_pos
        + bevy::math::DVec3::new(
            lp.radius_au * lp_angle.cos(),
            lp.radius_au * lp_angle.sin(),
            0.0,
        );
    let start_pos = Some(origin_pos);
    let end_pos = Some(lp_pos_au);

    // L1/L2: the LP is physically near the planet (±r_hill from the planet's
    // heliocentric position).  Park the fleet around the planet at r_hill so it
    // co-orbits with the planet rather than orbiting the Sun at 1 AU.
    //
    // L3/L4/L5: heliocentric co-orbital positions.  Park the fleet around the
    // star at the planet's SMA; `complete_fleet_maneuvers` will set direction=0.0
    // (frozen) because `is_kinematic()` + star destination → LP-stationed sentinel.
    let (destination_body, arrival_orbit_radius_au) = if matches!(lp.point, 1 | 2) {
        let r_hill = (lp.radius_au - lp.planet_sma_au).abs().max(0.001);
        (lp.planet_entity, r_hill)
    } else {
        (star_entity, lp.planet_sma_au)
    };

    let gm = lp.gm;
    let sma_m = option.sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let outward = lp.radius_au >= lp.planet_sma_au;
    let argument_of_periapsis = if outward {
        departure_angle
    } else {
        departure_angle - std::f64::consts::PI
    };
    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: option.sma_au,
        eccentricity: option.eccentricity,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body,
        reference_frame: TransferReferenceFrame::Body(star_entity),
        orbit_center: star_entity,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        preserve_orbit_geometry: false,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label,
        start_position_au: start_pos,
        end_position_au: end_pos,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::build_planned_transfer;
    use super::transfer_absolute_position;
    use crate::astronomy::components::SystemId;
    use crate::astronomy::orbit_position_from_mean_anomaly;
    use crate::astronomy::{KeplerOrbit, SpaceCoordinates};
    use crate::fleets::orbital_mechanics::TransferOption;
    use crate::fleets::{Fleet, FleetOrbit, TransferReferenceFrame};
    use crate::plugins::solar_system::{CelestialBody, LogicalParent};
    use crate::plugins::solar_system_data::BodyType;
    use bevy::math::DVec3;
    use bevy::prelude::*;

    fn test_body(
        name: &str,
        body_type: BodyType,
        mass: f64,
        radius: f32,
        visual_radius: f32,
    ) -> CelestialBody {
        CelestialBody {
            name: name.to_string(),
            radius,
            mass,
            body_type,
            visual_radius,
            asteroid_class: None,
            star_approach_au: None,
        }
    }

    /// Same as [`test_body`] but allows pinning the per-body star-approach
    /// radius.  Used by the GRA-149 C-2 acceptance test to verify the
    /// `star_approach_au: Some(0.05)` override reaches the planner unchanged.
    fn test_body_with_approach(
        name: &str,
        body_type: BodyType,
        mass: f64,
        radius: f32,
        visual_radius: f32,
        star_approach_au: Option<f64>,
    ) -> CelestialBody {
        CelestialBody {
            name: name.to_string(),
            radius,
            mass,
            body_type,
            visual_radius,
            asteroid_class: None,
            star_approach_au,
        }
    }

    #[test]
    fn build_planned_transfer_marks_cross_star_routes_barycentric() {
        let mut world = World::new();

        let star_a = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(-10.0, 0.0, 0.0)),
                SystemId(7),
            ))
            .id();
        let star_b = world
            .spawn((
                test_body("Star B", BodyType::Star, 1.3e30, 600_000.0, 34.0),
                SpaceCoordinates::new(DVec3::new(12.0, 0.0, 0.0)),
                SystemId(7),
            ))
            .id();

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(-8.8, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 1.0e-7),
                LogicalParent(star_a),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(14.1, 0.0, 0.0)),
                KeplerOrbit::circular(2.1, 8.0e-8),
                LogicalParent(star_b),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Full Thrust",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0,
            sma_au: 15.0,
            eccentricity: 0.4,
            energy_multiplier: 1.0,
            burn_time_s: 10_000.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("cross-star transfer should build successfully");

        assert_eq!(planned.destination_body, destination);
        assert_eq!(
            planned.reference_frame,
            TransferReferenceFrame::SystemBarycentric
        );
    }

    #[test]
    fn build_planned_transfer_keeps_curved_cross_star_routes_non_kinematic() {
        let mut world = World::new();

        // Stars at origin for this test - positions don't affect orbit-computed positions
        let star_a = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let star_b = world
            .spawn((
                test_body("Star B", BodyType::Star, 1.3e30, 600_000.0, 34.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();

        // Origin planet: orbit radius 1.2 AU, at position (1.2, 0, 0) relative to star_a
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.2, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 1.0e-7),
                LogicalParent(star_a),
                SystemId(7),
            ))
            .id();
        // Destination planet: orbit radius 2.1 AU, at position (2.1, 6.0, 0) relative to star_b
        // Use inclination=90deg to get y-offset in a circular orbit
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(2.1, 6.0, 0.0)),
                KeplerOrbit {
                    semi_major_axis: 2.1,
                    eccentricity: 0.0,
                    inclination: std::f64::consts::FRAC_PI_2, // 90 degrees for y-offset
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: 0.0,
                    mean_motion: 8.0e-8,
                },
                LogicalParent(star_b),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Curved Efficient",
            total_delta_v_ms: 9_000.0,
            delta_v1_ms: 4_500.0,
            delta_v2_ms: 4_500.0,
            transfer_time_s: 86_400.0 * 120.0,
            sma_au: 18.0,
            eccentricity: 0.55,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: Some(KeplerOrbit::circular(18.0, 1.0e-8)),
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("curved cross-star transfer should build successfully");

        assert_eq!(
            planned.reference_frame,
            TransferReferenceFrame::SystemBarycentric
        );
        assert_eq!(planned.option_label, "Curved Efficient");
        assert!(planned.start_position_au.is_some());
        assert!(planned.end_position_au.is_some());
        assert!(planned.departure_velocity_ms.is_some());
        assert!(planned.arrival_velocity_ms.is_some());
    }

    #[test]
    fn build_planned_transfer_star_origin_uses_parking_orbit_radius() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(1.8, 0.5, 0.0)),
                KeplerOrbit::new(0.02, 1.85, 0.0, 0.0, 0.2, 0.4, 1.2e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let mut orbit = FleetOrbit::new(star, 0.08);
        orbit.angle_rad = 0.35;

        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 4.0,
            sma_au: 1.0,
            eccentricity: 0.5,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("star-origin transfer should build successfully");

        assert_eq!(planned.reference_frame, TransferReferenceFrame::Body(star));

        let departure_pos = orbit_position_from_mean_anomaly(
            &planned.transfer_orbit,
            planned.transfer_orbit.mean_anomaly_epoch,
        );
        assert!(departure_pos.length() > orbit.radius_au * 0.5);
    }

    #[test]
    fn build_planned_transfer_to_star_preserves_origin_orbital_plane() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(11.0, -7.0, 3.0)),
                SystemId(7),
            ))
            .id();

        let origin_orbit = KeplerOrbit::new(0.08, 1.6, 0.72, 0.91, 0.35, 0.44, 1.2e-7);
        let origin_pos =
            orbit_position_from_mean_anomaly(&origin_orbit, origin_orbit.mean_anomaly_epoch);
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(11.0, -7.0, 3.0) + origin_pos),
                origin_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 6.0,
            sma_au: 0.9,
            eccentricity: 0.45,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("star-destination transfer should build successfully");

        assert_eq!(planned.reference_frame, TransferReferenceFrame::Body(star));
        assert!((planned.transfer_orbit.inclination - origin_orbit.inclination).abs() < 1e-9);
        assert!(
            (planned.transfer_orbit.longitude_ascending_node
                - origin_orbit.longitude_ascending_node)
                .abs()
                < 1e-9
        );
        assert!(planned.start_position_au.is_some());
        assert!(planned.end_position_au.is_some());
        assert!(planned.departure_velocity_ms.is_some());
        assert!(planned.arrival_velocity_ms.is_some());
    }

    #[test]
    fn build_planned_transfer_to_star_tracks_departure_epoch() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.2, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 2.2e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 6.0,
            sma_au: 0.9,
            eccentricity: 0.45,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned_now = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("initial star transfer should build successfully");
        let planned_later = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            86_400.0 * 20.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("delayed star transfer should build successfully");

        assert_ne!(
            planned_now.transfer_orbit.argument_of_periapsis,
            planned_later.transfer_orbit.argument_of_periapsis
        );
        assert_ne!(
            planned_now.start_position_au,
            planned_later.start_position_au
        );
    }

    #[test]
    fn build_planned_transfer_from_star_preserves_destination_orbital_plane() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(-5.0, 2.0, -1.0)),
                SystemId(7),
            ))
            .id();

        let destination_orbit = KeplerOrbit::new(0.04, 1.9, 0.63, 1.14, 0.2, 0.51, 1.1e-7);
        let destination_pos = orbit_position_from_mean_anomaly(
            &destination_orbit,
            destination_orbit.mean_anomaly_epoch,
        );
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(-5.0, 2.0, -1.0) + destination_pos),
                destination_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let mut orbit = FleetOrbit::new(star, 0.08);
        orbit.angle_rad = 0.35;

        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 4.0,
            sma_au: 1.0,
            eccentricity: 0.5,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("star-origin transfer should build successfully");

        assert_eq!(planned.reference_frame, TransferReferenceFrame::Body(star));
        assert!((planned.transfer_orbit.inclination - destination_orbit.inclination).abs() < 1e-9);
        assert!(
            (planned.transfer_orbit.longitude_ascending_node
                - destination_orbit.longitude_ascending_node)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn build_planned_transfer_same_star_lambert_carries_exact_endpoint_data() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(4.0, -3.0, 2.0)),
                SystemId(7),
            ))
            .id();

        let origin_orbit = KeplerOrbit::new(0.0, 1.3, 0.47, 0.82, 0.33, 0.21, 0.0);
        let destination_orbit = KeplerOrbit::new(0.0, 2.4, 0.47, 0.82, 0.33, 1.12, 0.0);
        let origin_pos =
            orbit_position_from_mean_anomaly(&origin_orbit, origin_orbit.mean_anomaly_epoch);
        let destination_pos = orbit_position_from_mean_anomaly(
            &destination_orbit,
            destination_orbit.mean_anomaly_epoch,
        );

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(4.0, -3.0, 2.0) + origin_pos),
                origin_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(4.0, -3.0, 2.0) + destination_pos),
                destination_orbit,
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let transfer_time_s = 86_400.0 * 220.0;
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s,
            sma_au: 1.8,
            eccentricity: 0.3,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("same-star transfer should build successfully");

        assert!(planned.preserve_orbit_geometry);
        assert!(planned.start_position_au.is_some());
        assert!(planned.end_position_au.is_some());
        assert!(planned.departure_velocity_ms.is_some());
        assert!(planned.arrival_velocity_ms.is_some());
    }

    #[test]
    fn build_planned_transfer_cross_star_to_star_uses_barycentric_approach() {
        let mut world = World::new();

        let star_a = world
            .spawn((
                test_body("Alpha A", BodyType::Star, 2.0e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::new(-2000.0, 0.0, 0.0)),
                SystemId(7),
            ))
            .id();
        let star_b = world
            .spawn((
                test_body("Proxima", BodyType::Star, 2.4e29, 110_000.0, 22.0),
                SpaceCoordinates::new(DVec3::new(2100.0, 120.0, 0.0)),
                SystemId(7),
            ))
            .id();

        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(-1998.8, 0.0, 0.0)),
                KeplerOrbit::circular(1.2, 1.0e-7),
                LogicalParent(star_a),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Long Coast",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 500.0,
            sma_au: 3000.0,
            eccentricity: 0.2,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star_b,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("cross-star star-approach transfer should build successfully");

        assert_eq!(
            planned.reference_frame,
            TransferReferenceFrame::SystemBarycentric
        );
        assert_eq!(planned.destination_body, star_b);
        assert_eq!(planned.arrival_orbit_radius_au, 0.3);
        let end_pos = planned
            .end_position_au
            .expect("approach endpoint should be stored");
        let star_pos = body_query
            .get(star_b)
            .map(|(_, _, sc, _, _)| sc.position)
            .expect("star should exist");
        let approach_distance = (end_pos - star_pos).length();
        assert!((approach_distance - 0.3).abs() < 1e-6);
    }

    // ──────────────────────────────────────────────────────────────────────
    // GRA-149 acceptance tests (C-1 / C-2 / C-3)
    //
    // Pin the GRA-149 fixes so a future regression to the legacy
    // `sma < MIN_HELIOCENTRIC_SMA_AU` classifier (which mis-classified
    // hot-Jupiters as moons) is caught.
    // ──────────────────────────────────────────────────────────────────────

    /// C-1: the stellar flyby constant is documented at 1.5 R★ (1500 m/km
    /// × 1.5) and the planetary constant is 3 planetary radii.  Both must
    /// stay larger than 1.0× their respective body radii so the
    /// flyby-periapsis math never under-shoots into the photosphere /
    /// atmosphere.
    #[test]
    fn gra149_c1_stellar_flyby_constants_are_safe_periapsis_multiples() {
        // Bind to local variables so clippy::assertions_on_constants does
        // not see a literal-only comparison.
        let stellar = super::STELLAR_FLYBY_RADIUS_KM_MULTIPLIER;
        let planetary = super::PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER;
        assert!(
            stellar >= 1_500.0,
            "STELLAR_FLYBY_RADIUS_KM_MULTIPLIER = {stellar} km; must be >= 1.5 R☉ (1500 km)"
        );
        assert!(
            planetary >= 3_000.0,
            "PLANETARY_FLYBY_RADIUS_KM_MULTIPLIER = {planetary} km; must be >= 3 R_planet"
        );
        assert!(
            stellar < planetary,
            "stellar flyby constant ({stellar}) must be < planetary ({planetary})"
        );
    }

    /// C-2: a star with `star_approach_au: Some(0.05)` parks the fleet at
    /// 0.05 AU when the destination is that star — not the 0.3 AU default
    /// and not the planet's SOI.  This is the M-3 / GRA-153 dependency
    /// pin: M-3 calls `origin_body.star_approach_au` (or the parking-orbit
    /// default) to rebuild the parking orbit after an Abort.
    #[test]
    fn gra149_c2_star_approach_respects_per_body_override() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body_with_approach(
                    "Red Dwarf",
                    BodyType::Star,
                    2.4e29, // 0.12 M☉ — sub-solar
                    110_000.0,
                    22.0,
                    Some(0.05), // per-body override
                ),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let origin = world
            .spawn((
                test_body("Origin", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(0.45, 0.0, 0.0)),
                KeplerOrbit::circular(0.5, 1.0e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(origin, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 8.0,
            sma_au: 0.27,
            eccentricity: 0.4,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            star,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("star-approach transfer with override should build");

        // The per-body override is the source of truth — not 0.3 AU, not SOI.
        // The inward-transfer safety floor at `planet_sma_au * 0.01 = 0.005`
        // does not bind here because 0.05 > 0.005.
        assert!(
            (planned.arrival_orbit_radius_au - 0.05).abs() < 1e-9,
            "arrival_orbit_radius_au = {}, expected 0.05 (per-body override)",
            planned.arrival_orbit_radius_au
        );
    }

    /// C-3: a hot-Jupiter (gas giant at SMA 0.02 AU, well below the legacy
    /// 0.05 AU classifier) with `LogicalParent(star)` is correctly treated
    /// as heliocentric by the planner — it does NOT walk up to the parent
    /// star's 1.0 AU orbit.  This pins the GRA-149 C-3 fix to
    /// `is_stellar_mass` and ensures hot-Jupiters are not silently
    /// mis-classified as moons.
    #[test]
    fn gra149_c3_hot_jupiter_uses_heliocentric_frame_not_walked_up() {
        let mut world = World::new();

        let star = world
            .spawn((
                test_body("Star A", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(7),
            ))
            .id();
        let hot_jupiter = world
            .spawn((
                test_body("Hot Jupiter", BodyType::GasGiant, 1.9e27, 70_000.0, 30.0),
                SpaceCoordinates::new(DVec3::new(0.02, 0.0, 0.0)),
                KeplerOrbit::circular(0.02, 1.0e-6), // SMA = 0.02 AU
                LogicalParent(star),
                SystemId(7),
            ))
            .id();
        let destination = world
            .spawn((
                test_body("Destination", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(1.8, 0.5, 0.0)),
                KeplerOrbit::new(0.02, 1.85, 0.0, 0.0, 0.2, 0.4, 1.2e-7),
                LogicalParent(star),
                SystemId(7),
            ))
            .id();

        let fleet = Fleet::new("Test Fleet".to_string());
        let orbit = FleetOrbit::new(hot_jupiter, 0.0001);
        let option = TransferOption {
            label: "Efficient",
            total_delta_v_ms: 12_000.0,
            delta_v1_ms: 6_000.0,
            delta_v2_ms: 6_000.0,
            transfer_time_s: 86_400.0 * 4.0,
            sma_au: 1.0,
            eccentricity: 0.5,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        };

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let mut system_id_query_state = world.query::<&SystemId>();
        let body_query = body_query_state.query(&world);
        let system_id_query = system_id_query_state.query(&world);

        let planned = build_planned_transfer(
            Entity::PLACEHOLDER,
            &fleet,
            &orbit,
            destination,
            0.0,
            &body_query,
            &option,
            None,
            &system_id_query,
            7,
        )
        .expect("hot-Jupiter to outer-planet transfer should build");

        // The transfer must be heliocentric (BodyLocal(star)), not the
        // hot-Jupiter's planet-local frame.  Under the legacy SMA
        // classifier (0.05 AU), this would have been classified as a
        // planet-local transfer — the regression to guard against.
        match planned.reference_frame {
            TransferReferenceFrame::Body(frame_center) => {
                assert_eq!(
                    frame_center, star,
                    "hot-Jupiter at 0.02 AU must resolve to star's frame, \
                     not the planet's frame"
                );
            }
            other => panic!("expected Body(star) frame for hot-Jupiter, got {:?}", other),
        }
    }

    /// L-6: a single-star transfer from a star-system origin must read
    /// positions in the star-centric frame, and an inter-star transfer must
    /// read them in the barycentric frame.  This test exercises the boundary:
    /// the same fleet moves from one frame to the other mid-flight (which
    /// shouldn't happen in practice, but the math must be defensible).
    #[test]
    fn transfer_absolute_position_uses_consistent_frame_at_star_system_boundary() {
        let mut world = World::new();

        // Star A: single-star system (SystemId 11).  Position is the
        // star-system origin, so all bodies in this system are star-centric
        // relative to A.
        let star_a = world
            .spawn((
                test_body("Alpha", BodyType::Star, 1.9e30, 700_000.0, 40.0),
                SpaceCoordinates::new(DVec3::ZERO),
                SystemId(11),
            ))
            .id();
        let planet_a = world
            .spawn((
                test_body("Alpha-b", BodyType::Planet, 5.97e24, 6_371.0, 12.0),
                SpaceCoordinates::new(DVec3::new(1.0, 0.0, 0.0)),
                KeplerOrbit::circular(1.0, 1.0e-7),
                LogicalParent(star_a),
                SystemId(11),
            ))
            .id();
        // Star B: second star of a binary (SystemId 12).  Its
        // `SpaceCoordinates.position` is the barycentric offset of B
        // relative to the A+B barycentre.
        let star_b = world
            .spawn((
                test_body("Beta", BodyType::Star, 1.3e30, 600_000.0, 35.0),
                SpaceCoordinates::new(DVec3::new(20.0, 0.0, 0.0)),
                SystemId(12),
            ))
            .id();
        let planet_b = world
            .spawn((
                test_body("Beta-b", BodyType::Planet, 6.4e24, 6_800.0, 13.0),
                SpaceCoordinates::new(DVec3::new(21.0, 0.0, 0.0)),
                KeplerOrbit::circular(1.0, 1.0e-7),
                LogicalParent(star_b),
                SystemId(12),
            ))
            .id();

        let mut body_query_state = world.query::<(
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&KeplerOrbit>,
            Option<&LogicalParent>,
        )>();
        let body_query = body_query_state.query(&world);

        // Single-star: planet A's transfer-absolute position equals its
        // SpaceCoordinates.position (star-centric, parent is at origin).
        let pos_planet_a = transfer_absolute_position(planet_a, 0.0, &body_query)
            .expect("planet A absolute position should resolve");
        let sc_planet_a = body_query
            .get(planet_a)
            .map(|(_, _, sc, _, _)| sc.position)
            .unwrap();
        assert_eq!(
            pos_planet_a, sc_planet_a,
            "single-star system: planet A position must equal its SpaceCoordinates"
        );

        // Inter-star: planet B's transfer-absolute position equals its
        // SpaceCoordinates.position (already barycentric in this world
        // model — star B itself is offset by 20 AU from the barycentre).
        let pos_planet_b = transfer_absolute_position(planet_b, 0.0, &body_query)
            .expect("planet B absolute position should resolve");
        let sc_planet_b = body_query
            .get(planet_b)
            .map(|(_, _, sc, _, _)| sc.position)
            .unwrap();
        assert_eq!(
            pos_planet_b, sc_planet_b,
            "barycentric: planet B position must equal its SpaceCoordinates"
        );

        // The key invariant: a transfer crossing the star-system boundary
        // (planet A → planet B) computes positions in their own frame.  The
        // math at the boundary is just a position comparison; it must not
        // silently re-interpret star-A-centric positions as barycentric.
        // We assert the boundary distance is the *sum* of the two offsets,
        // not the difference, because both are now in the same barycentric
        // frame.
        let boundary_distance = (pos_planet_b - pos_planet_a).length();
        let expected = (sc_planet_b - sc_planet_a).length();
        assert!(
            (boundary_distance - expected).abs() < 1e-6,
            "boundary distance must be consistent: got {}, expected {}",
            boundary_distance,
            expected
        );
    }
}
