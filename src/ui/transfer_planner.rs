use super::*;
use super::time::format_timestamp_date_time;

pub(super) fn render_transfer_planner(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    current_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    all_fleets_query: &Query<(Entity, &Fleet, &SpaceCoordinates, Option<&FleetOrbit>, Option<&ActiveManeuver>), Without<CelestialBody>>,
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
        Body { entity: Entity, name: String },
        // Rings are treated like regular bodies for selection; the extra
        // parent/radius information used to be stored here but never read.
        Ring { entity: Entity, name: String },
        // TODO(lagrange-transfers): variant kept so the match arm compiles; re-enable construction when ready.
        #[allow(dead_code)]
        Lagrange { lp: LagrangeTarget },
        FleetTarget { entity: Entity, name: String, in_transit: bool },
        StarSystem { system_id: usize, name: String, distance_ly: f32 },
    }

    let mut dest_entries: Vec<DestEntry> = Vec::new();

    // Collect all valid candidate bodies (exclude Star, include Ring)
    // For Rings: sma = None (no KeplerOrbit); radius stored via body.radius field separately.
    let candidates: Vec<(Entity, String, BodyType, Option<f64>, Option<Entity>)> = body_query
        .iter()
        .filter_map(|(e, body, _, maybe_ko, maybe_lp)| {
            if e == orbit.body { return None; }
            if body.body_type == BodyType::Star { return None; }
            if !body_system_ids.get(e).ok().map(|s| s.0 == current_system_id).unwrap_or(false) {
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
            if body.body_type != BodyType::Ring { return None; }
            if !body_system_ids.get(e).ok().map(|s| s.0 == current_system_id).unwrap_or(false) {
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
        let orbit_body_name = body_query.get(orbit.body)
            .map(|(_, b, _, _, _)| b.name.clone()).unwrap_or_default();
        let mut local: Vec<(Entity, String, f64)> = candidates.iter()
            .filter(|(_, _, btype, _, parent)| {
                *parent == Some(orbit.body) && *btype != BodyType::Ring
            })
            .filter_map(|(e, name, _, sma, _)| sma.map(|s| (*e, name.clone(), s)))
            .collect();
        // Rings around the current orbit body
        let mut local_rings: Vec<(Entity, String, Option<Entity>, f64)> = ring_candidates.iter()
            .filter(|(_, _, parent, _)| *parent == Some(orbit.body))
            .cloned().collect();
        if !local.is_empty() || !local_rings.is_empty() {
            local.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            local_rings.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            dest_entries.push(DestEntry::Header(format!("{orbit_body_name} System")));
            for (e, name, _) in &local {
                dest_entries.push(DestEntry::Body { entity: *e, name: name.clone() });
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
    let mut planet_map: std::collections::BTreeMap<String, (Entity, f64, Vec<(Entity, String, f64, bool)>)> =
        std::collections::BTreeMap::new();

    // Regular moons / small bodies orbiting a planet
    for (e, name, btype, sma, parent) in &candidates {
        if *btype == BodyType::Ring { continue; }
        let parent_e = match parent { Some(p) => *p, None => continue };
        if parent_e == orbit.body { continue; }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star { continue; }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            if let Some(s) = sma {
                planet_map.entry(pb.name.clone())
                    .or_insert_with(|| (parent_e, parent_sma, vec![]))
                    .2.push((*e, name.clone(), *s, false)); // false = not a ring
            }
        }
    }
    // Rings orbiting a planet that isn't the fleet's body
    for (e, name, parent_opt, radius_au) in &ring_candidates {
        let parent_e = match parent_opt { Some(p) => *p, None => continue };
        if parent_e == orbit.body { continue; }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star { continue; }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            planet_map.entry(pb.name.clone())
                .or_insert_with(|| (parent_e, parent_sma, vec![]))
                .2.push((*e, name.clone(), *radius_au, true)); // true = ring
        }
    }

    let mut sorted_planet_systems: Vec<_> = planet_map.into_iter().collect();
    sorted_planet_systems.sort_by(|a, b| a.1.1.partial_cmp(&b.1.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut planets_shown = std::collections::HashSet::<Entity>::new();
    for (planet_name, (parent_e, _parent_sma, mut children)) in sorted_planet_systems {
        planets_shown.insert(parent_e);
        children.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header(format!("{planet_name} System")));
        if orbit.body != parent_e {
            dest_entries.push(DestEntry::Body { entity: parent_e, name: planet_name.clone() });
        }
        for (e, name, _sma, is_ring) in &children {
            if *is_ring {
                dest_entries.push(DestEntry::Ring {
                    entity: *e,
                    name: name.clone(),
                });
            } else {
                dest_entries.push(DestEntry::Body { entity: *e, name: name.clone() });
            }
        }
        // TODO(lagrange-transfers): Re-enable planet and moon Lagrange point
        // sub-groups in this dropdown once transfer planning is working.
    }

    // ── Group: Planets/GasGiants not yet shown (no children found in data) ───
    let already_listed: std::collections::HashSet<Entity> = dest_entries.iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();

    let mut standalone: Vec<(Entity, String, f64)> = candidates.iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet)
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

    // ── Group: Small bodies ─────────────────────────────────────────────────
    let already_listed2: std::collections::HashSet<Entity> = dest_entries.iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();
    let mut small_bodies: Vec<(Entity, String, f64)> = candidates.iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Asteroid | BodyType::Comet)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !small_bodies.is_empty() {
        small_bodies.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let sb_label = if small_bodies.len() > 5 {
            format!("Small Bodies ({} total)", small_bodies.len())
        } else {
            "Small Bodies".to_string()
        };
        dest_entries.push(DestEntry::Header(sb_label));
        for (e, name, _) in small_bodies {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Solar Approach ────────────────────────────────────────────────
    // Always offer a direct solar-approach destination so the player can plot
    // an inward heliocentric transfer toward the star.  Filter by current_system_id
    // to find Sol, not Alpha Centauri or another star from a different system.
    let star_entity = body_query.iter()
        .find(|(e, b, _, _, _)| {
            b.body_type == BodyType::Star
                && body_system_ids.get(*e).ok().map(|s| s.0 == current_system_id).unwrap_or(false)
        })
        .map(|(e, _, _, _, _)| e);
    if let Some(star_e) = star_entity {
        dest_entries.push(DestEntry::Header("Solar".to_string()));
        dest_entries.push(DestEntry::Body {
            entity: star_e,
            name: "☀ Solar Approach (0.3 AU)".to_string(),
        });
    }

    // ── Group: Interstellar ──────────────────────────────────────────────────
    // List every other star system from NearbyStarsData as an interstellar target.
    // The current system is identified by its numeric id; Sol = id 0 by convention.
    {
        let mut interstellar_entries: Vec<DestEntry> = nearby_stars.systems
            .iter()
            .filter(|sys| {
                // Exclude the current system (id comparison via name match is a fallback)
                // NearbyStarsData systems use 0-based index ordering; system_id 0 = Sol.
                // We exclude any system whose name matches current system's star name.
                let this_star_name = body_query.iter()
                    .find(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids.get(*e).ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| b.name.as_str())
                    .unwrap_or("Sol");
                // Each StarSystemData has stars[0].name; compare to current star
                !sys.stars.iter().any(|s| s.name == this_star_name)
                    && sys.distance_ly > 0.0
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
                let da = if let DestEntry::StarSystem { distance_ly, .. } = a { *distance_ly } else { 0.0 };
                let db = if let DestEntry::StarSystem { distance_ly, .. } = b { *distance_ly } else { 0.0 };
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
            groups.push(DestGroup { name, entries: Vec::new() });
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
            let mut fleet_group = DestGroup { name: "Fleets".to_string(), entries: Vec::new() };
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
                DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => *entity == target,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(ref lp) = fleet_ui_state.target_lagrange {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Lagrange { lp: entry_lp } => entry_lp.point == lp.point && entry_lp.planet_entity == lp.planet_entity,
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
    let cat_label = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    }).map(|g| g.name.clone()).unwrap_or_else(|| fleet_ui_state.selected_dest_category.clone().unwrap_or_else(|| "— System —".to_owned()));

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("System:").size(13.0));
        egui::ComboBox::from_id_salt("fleet_dest_category")
            .selected_text(&cat_label)
            .width(200.0)
            .show_ui(ui, |ui| {
                for group in &groups {
                    let sel = fleet_ui_state.selected_dest_category.as_deref();
                    let cat_is_sel = sel == Some(&group.name) || (sel == Some("Small Bodies") && group.name.starts_with("Small Bodies"));
                    if ui.selectable_label(
                        cat_is_sel,
                        egui::RichText::new(&group.name).size(13.0),
                    ).clicked() && !cat_is_sel {
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
        all_fleets_query.get(tf)
            .map(|(_, f, _, _, ma)| {
                let status = if ma.is_some() { "✈" } else { "🛰" };
                format!("{status} {}", f.name)
            })
            .unwrap_or_else(|_| "— Target —".to_owned())
    } else if let Some((_, ref name, _)) = fleet_ui_state.target_star_system {
        name.clone()
    } else {
        fleet_ui_state.target_body
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
                                    if !first_sub { ui.add_space(4.0); }
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
                                    if ui.selectable_label(
                                        selected,
                                        egui::RichText::new(format!("  {name}")).size(12.0),
                                    ).clicked() && !selected {
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
                                    if ui.selectable_label(
                                        selected,
                                        egui::RichText::new(format!("  {name} 💍")).size(12.0),
                                    ).clicked() && !selected {
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
                                DestEntry::FleetTarget { entity, name, in_transit } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_fleet == Some(*entity);
                                    let icon = if *in_transit { "✈" } else { "🛰" };
                                    let status = if *in_transit { "In transit" } else { "In orbit" };
                                    let label = format!("  {icon} {name}  ({status})");
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(label)
                                            .size(12.0)
                                            .color(theme::ACCENT),
                                    ).clicked() && !is_sel {
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
                                DestEntry::StarSystem { system_id, name, distance_ly } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_star_system
                                        .as_ref().map(|(id, _, _)| *id == *system_id)
                                        .unwrap_or(false);
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(format!("  {name}"))
                                            .size(12.0)
                                            .color(theme::GRAVITY_ASSIST),
                                    ).clicked() && !is_sel {
                                        fleet_ui_state.target_star_system = Some((*system_id, name.clone(), *distance_ly));
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
                if ui.add(
                    egui::Slider::new(&mut pd, 0.0_f32..=1_000.0_f32)
                        .suffix(" km")
                        .text("0 = rendezvous")
                        .step_by(10.0),
                ).changed() {
                    fleet_ui_state.intercept_passing_km = pd as f64;
                    fleet_ui_state.computed_options.clear();
                }
            });

            // Encounter speed: 0 = match velocity (boarding), up to 30 km/s = high-speed pass
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Encounter speed:").size(12.0));
                let mut spd_kms = (fleet_ui_state.intercept_speed_ms / 1_000.0) as f32;
                if ui.add(
                    egui::Slider::new(&mut spd_kms, 0.0_f32..=30.0_f32)
                        .suffix(" km/s")
                        .text("0 = match velocity")
                        .step_by(0.5),
                ).changed() {
                    fleet_ui_state.intercept_speed_ms = spd_kms as f64 * 1_000.0;
                    fleet_ui_state.computed_options.clear();
                }
            });

            ui.label(
                egui::RichText::new(
                    if fleet_ui_state.intercept_passing_km < 1.0 && fleet_ui_state.intercept_speed_ms < 100.0 {
                        "Mode: Rendezvous / docking approach"
                    } else if fleet_ui_state.intercept_passing_km > 100.0 || fleet_ui_state.intercept_speed_ms > 5_000.0 {
                        "Mode: High-speed flyby (combat pass)"
                    } else {
                        "Mode: Close approach (boarding range)"
                    }
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
            let target_sc = all_fleets_query.get(target_fleet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(bevy::math::DVec3::ZERO);
            let r2_au = target_sc.length().max(0.001);

            // r1: heliocentric distance of the departing fleet
            let r1_au = {
                let own_ko = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let origin_parent = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);
                if own_ko.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_ko)
                        .unwrap_or(1.0)
                } else {
                    own_ko.unwrap_or(1.0)
                }
            };
            fleet_ui_state.computed_options = calculate_transfer_options(r1_au, r2_au, GM_SUN, 0.0);
            // Post-process: fill burn_time_s and flag thrust-limited options.
            apply_thrust_limits(
                &mut fleet_ui_state.computed_options,
                fleet.min_accel_ms2(),
                fleet.average_isp_s(),
            );
            // Add kinematic options for high-thrust fleets intercepting other fleets.
            let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
            let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
            let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
            let d = (r2_au - r1_au).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
            let mut kinematics = kinematic_transfer_options(
                d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                hohmann_dv, sma_h, ecc_h, false
            );
            fleet_ui_state.computed_options.append(&mut kinematics);
        } else if let Some(target_entity) = body_target_snap {
            //   - Ring transfer (dest has no KeplerOrbit; use body.radius as r2):
            //       r1 = fleet orbit radius or origin SMA, r2 = ring.radius_au, GM = parent mass * G
            //   - Local transfer (dest orbits fleet's body, e.g. Earth→Moon):
            //       r1 = fleet's parking orbit radius, r2 = dest SMA, GM = parent mass * G
            //   - Moon-to-moon (both orbit the same planet):
            //       r1 = origin moon SMA, r2 = dest moon SMA, GM = shared planet mass * G
            //   - Solar approach (dest is a star):
            //       r1 = fleet's heliocentric SMA, r2 = 0.3 AU, GM = GM_SUN
            //   - Heliocentric transfer (both in heliocentric orbits):
            //       r1 = origin body heliocentric SMA, r2 = dest heliocentric SMA, GM_SUN
            let dest_body_type = body_query.get(target_entity).ok()
                .map(|(_, b, _, _, _)| b.body_type);
            let dest_has_orbit = body_query.get(target_entity).ok()
                .and_then(|(_, _, _, ko, _)| ko).is_some();
            let dest_parent = body_query.get(target_entity).ok()
                .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);
            let origin_parent = body_query.get(orbit.body).ok()
                .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);

            // Target solar approach orbit (AU from star).  Inside Mercury's orbit so the
            // transfer is always clearly "inward".  Requires advanced propulsion (~10–20 km/s).
            const SOLAR_APPROACH_AU: f64 = 0.3;

            let (r1, r2, gm) = if dest_body_type == Some(BodyType::Star) {
                // Heliocentric inward transfer: plot a Hohmann from the fleet's heliocentric
                // distance to SOLAR_APPROACH_AU using GM_SUN as the central-body parameter.
                // Walk up the parent chain to find the fleet's heliocentric SMA.
                let own_sma = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r1_au = if own_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Fleet is parked at a moon/sub-body; use its planet's heliocentric SMA.
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                } else {
                    own_sma.unwrap_or(1.0)
                };
                // Ensure r2 is strictly less than r1 (always an inward transfer).
                let r2_au = SOLAR_APPROACH_AU.min(r1_au * 0.5);
                (r1_au, r2_au, GM_SUN)
            } else if !dest_has_orbit && dest_parent == Some(orbit.body) {
                // Ring around current orbit body
                let parent_mass = body_query.get(orbit.body).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if !dest_has_orbit && dest_parent.is_some() && dest_parent == origin_parent {
                // Ring around another planet (dest_parent is a planet, not fleet's body)
                let shared = dest_parent.unwrap();
                let parent_mass = body_query.get(shared).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (r1, r2, G_CONST * parent_mass)
            } else if dest_parent == Some(orbit.body) {
                // Local: destination orbits the fleet's current body
                let parent_mass = body_query.get(orbit.body).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r2 = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if dest_parent.is_some() && dest_parent == origin_parent {
                // Both orbit the same central body (moon-to-moon, OR interplanetary e.g. Earth→Mars)
                let shared = dest_parent.unwrap();
                // NOTE: The Sun lacks SpaceCoordinates, so body_query.get(Sun) fails.
                // Fall back to GM_SUN so interplanetary transfers compute correctly.
                let gm = body_query.get(shared).ok()
                    .map(|(_, b, _, _, _)| {
                        if b.body_type == BodyType::Star { GM_SUN } else { G_CONST * b.mass }
                    })
                    .unwrap_or(GM_SUN);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                let r2 = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                (r1, r2, gm)
            } else if Some(target_entity) == origin_parent {
                // Downward transfer: fleet is at a moon, destination is the parent planet.
                // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
                let parent_mass = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                // Park at ~3× destination body surface radius (low orbit).
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 3_000.0) / AU_IN_METERS)
                    .unwrap_or(4.26e-5);
                (r1, r2.min(r1 * 0.5), G_CONST * parent_mass)
            } else {
                // Heliocentric: fleet is at a body that is not in the same parent chain as dest.
                // If fleet is parked at a moon, its KeplerOrbit SMA is Earth-relative, NOT
                // heliocentric. Walk up one level to get the heliocentric SMA.
                let own_sma = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r1 = if own_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Small SMA → likely a moon; use its parent's heliocentric SMA
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                } else {
                    own_sma.unwrap_or(1.0)
                };
                let dest_sma = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r2 = if dest_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Small SMA → likely a moon; use its parent's heliocentric SMA
                    dest_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(dest_sma)
                        .unwrap_or(1.5)
                } else {
                    dest_sma.unwrap_or(1.5)
                };
                (r1, r2, GM_SUN)
            };
            // For course corrections, compute the fleet's position in the correct local frame.
            // For heliocentric transfers the position is already relative to the Sun.
            // For local transfers (e.g. moon-to-moon around Jupiter) we must subtract
            // the central body's heliocentric position so distances and phase angles
            // are Jupiter-centric, not Sun-centric.
            let is_heliocentric_gm = (gm - GM_SUN).abs() < 1e10;
            let cc_local_pos: Option<bevy::math::DVec3> = if is_course_correction {
                if let Some(fleet_helio) = course_correction_sc {
                    if is_heliocentric_gm {
                        Some(fleet_helio)
                    } else {
                        // Determine the central body entity: shared parent of both moons,
                        // or the planet if the fleet is going to one of its moons.
                        let central_entity = dest_parent.or(origin_parent);
                        let center_helio = central_entity
                            .and_then(|e| body_query.get(e).ok())
                            .map(|(_, _, sc, _, _)| sc.position)
                            .unwrap_or(bevy::math::DVec3::ZERO);
                        Some(fleet_helio - center_helio)
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
            fleet_ui_state.computed_options = {
                // Extract angles of origin and destination bodies in the correct coordinate system.
                let is_heliocentric = (gm - GM_SUN).abs() < 1e10;
                // Moon → parent-planet case: target IS the body that origin orbits around.
                let is_moon_to_parent = Some(target_entity) == origin_parent;

                let get_heliocentric_pos = |entity: Entity| -> Option<bevy::math::DVec3> {
                    let entry = body_query.get(entity).ok()?;
                    let is_moon = entry.1.body_type == BodyType::Moon;
                    if is_moon {
                        let parent_entity = entry.4?.0;
                        let parent_entry = body_query.get(parent_entity).ok()?;
                        Some(parent_entry.2.position)
                    } else {
                        Some(entry.2.position)
                    }
                };

                let get_local_pos = |entity: Entity, central_body: Entity| -> Option<bevy::math::DVec3> {
                    if entity == central_body {
                        Some(bevy::math::DVec3::ZERO)
                    } else {
                        let entry = body_query.get(entity).ok()?;
                        Some(entry.2.position)
                    }
                };

                let (pos1, pos2) = if is_moon_to_parent {
                    // Moon→parent: use Moon's position relative to the parent planet.
                    // The parent planet is at the centre of the local frame.
                    let moon_helio = body_query.get(orbit.body).ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    let planet_helio = body_query.get(target_entity).ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    (Some(moon_helio - planet_helio), Some(bevy::math::DVec3::ZERO))
                } else if is_heliocentric {
                    (get_heliocentric_pos(orbit.body), get_heliocentric_pos(target_entity))
                } else {
                    let central_body = dest_parent.unwrap_or(orbit.body);
                    (get_local_pos(orbit.body, central_body), get_local_pos(target_entity, central_body))
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
                    let origin_ko = body_query.get(orbit.body).ok().and_then(|(_, _, _, ko, _)| ko);
                    let dest_ko   = body_query.get(target_entity).ok().and_then(|(_, _, _, ko, _)| ko);

                    if dest_body_type == Some(BodyType::Star) || Some(target_entity) == origin_parent {
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
                                o.inclination, o.longitude_ascending_node,
                                d.inclination, d.longitude_ascending_node,
                            ),
                            _ => 0.0,
                        }
                    } else {
                        // Heliocentric: walk up from moons to their heliocentric parents.
                        let helio_origin_ko = if origin_ko.map(|ko| ko.semi_major_axis < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                            origin_parent.and_then(|pe| body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko))
                        } else {
                            origin_ko
                        };
                        let helio_dest_ko = if dest_ko.map(|ko| ko.semi_major_axis < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                            dest_parent.and_then(|pe| body_query.get(pe).ok().and_then(|(_, _, _, ko, _)| ko))
                        } else {
                            dest_ko
                        };
                        match (helio_origin_ko, helio_dest_ko) {
                            (Some(o), Some(d)) => plane_change_angle(
                                o.inclination, o.longitude_ascending_node,
                                d.inclination, d.longitude_ascending_node,
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
                            if let (Some(start), Some(end)) = (man.start_position_au, man.end_position_au) {
                                let dir   = (end - start).normalize_or_zero();
                                let dist_m = (end - start).length()
                                    * crate::fleets::orbital_mechanics::AU_IN_METERS;
                                let dur_s  = (man.arrival_time - man.departure_time).max(1.0);
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
                    let r_vec = cc_local_pos
                        .unwrap_or_else(|| bevy::math::DVec3::new(r1, 0.0, 0.0));
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
                if !is_course_correction {
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (r2 - r1).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, accel, fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                }
            }
            // ── Gravity assist candidates (heliocentric transfers only) ─────────
            // Collect planets between r1 and r2, compute two-leg patched-conic options.
            // Only meaningful when GM ≈ GM_SUN (genuinely heliocentric transfer).
            if (gm - GM_SUN).abs() < 1e10 && !is_course_correction {
                let ga_bodies: Vec<(String, f64, f64, f64)> = body_query
                    .iter()
                    .filter_map(|(e, body, _, maybe_ko, _)| {
                        if !matches!(body.body_type,
                            BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet)
                        { return None; }
                        // Exclude the fleet's current body and the chosen destination
                        if e == orbit.body || Some(e) == body_target_snap { return None; }
                        // Only consider planets/bodies in the current star system
                        if body_system_ids.get(e).map(|s| s.0).unwrap_or(0) != current_system_id {
                            return None;
                        }
                        let sma = maybe_ko?.semi_major_axis;
                        if sma < MIN_HELIOCENTRIC_SMA_AU { return None; }
                        let gm_p = G_CONST * body.mass;
                        // Safe flyby periapsis: 3 × body radius (km → m → AU)
                        let min_peri = (body.radius as f64 * 3_000.0) / AU_IN_METERS;
                        Some((body.name.clone(), sma, gm_p, min_peri.max(1e-6)))
                    })
                    .collect();

                let new_candidates: Vec<GravityAssistEntry> =
                    find_gravity_assist_options(r1, r2, gm, &ga_bodies)
                    .into_iter()
                    .filter_map(|opt| {
                        // Resolve each candidate to its ECS entity by name
                        let entity = body_query
                            .iter()
                            .find(|(_, b, _, _, _)| b.name == opt.body_name)
                            .map(|(e, _, _, _, _)| e)?;
                        Some(GravityAssistEntry { option: opt, flyby_entity: entity })
                    })
                    .collect();

                fleet_ui_state.gravity_assist_candidates = new_candidates;

                // Validate selected index is still in-range (target may have changed)
                if fleet_ui_state.selected_gravity_assist
                    .map(|i| i >= fleet_ui_state.gravity_assist_candidates.len())
                    .unwrap_or(false)
                {
                    fleet_ui_state.selected_gravity_assist = None;
                }
            } else {
                fleet_ui_state.gravity_assist_candidates.clear();
                fleet_ui_state.selected_gravity_assist = None;
            }

            // If a gravity assist is selected, prepend it as option 0 so the
            // regular execute/select logic treats it uniformly.
            if let Some(sel_ga) = fleet_ui_state.selected_gravity_assist {
                let ga_data = fleet_ui_state.gravity_assist_candidates.get(sel_ga)
                    .map(|e| (
                        e.option.total_dv_ms,
                        e.option.total_time_s,
                        e.option.flyby_radius_au,
                        e.option.dv_depart_ms + e.option.dv_mid_ms, // departure + mid-course
                        e.option.dv_arrive_ms,
                    ));
                if let Some((total_dv, total_time, fly_r, dv1, dv2)) = ga_data {
                    // Use Leg-1 Hohmann parameters (origin → flyby body) for the
                    // transfer-orbit Keplerian arc.  This makes the purple active-orbit
                    // arc match the approach leg shown in the gravity-assist preview.
                    // The arc is computed pointing from the origin toward the flyby body,
                    // and build_planned_transfer is passed the flyby entity as its orbital
                    // target so the departure/arrival plane vectors are consistent.
                    let (_, _, _, ga_sma, ga_ecc) = hohmann_transfer(r1, fly_r, gm);
                    let burn_t = compute_burn_time_s(total_dv, fleet.min_accel_ms2(), fleet.average_isp_s());
                    // Gravity-assist options use multi-leg patched-conic timing; the burn
                    // is spread across two legs so we apply the thrust-limit check here.
                    let (ga_transfer_time, ga_thrust_limited) = if burn_t > 0.0 && burn_t > total_time {
                        (burn_t, true)
                    } else {
                        (total_time, false)
                    };
                    let ga_option = TransferOption {
                        label: "Gravity Assist",
                        total_delta_v_ms: total_dv,
                        delta_v1_ms: dv1,   // actual departure + any mid-course burn
                        delta_v2_ms: dv2,   // actual arrival circularisation
                        plane_change_dv_ms: 0.0, // gravity-assist paths are heliocentric (ecliptic)
                        transfer_time_s: ga_transfer_time,
                        sma_au: ga_sma,     // Leg-1 ellipse SMA (origin → flyby body)
                        eccentricity: ga_ecc,
                        energy_multiplier: 1.0,
                        burn_time_s: burn_t,
                        is_thrust_limited: ga_thrust_limited,
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
                let r1_lp = body_query.get(orbit.body).ok()
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
                        body_query.get(orbit.body).ok()
                            .and_then(|(_, _, _, _, parent)| parent)
                            .and_then(|lpp| body_query.get(lpp.0).ok()
                                .and_then(|(_, _, _, ko, _)| ko)
                                .map(|ko| ko.semi_major_axis))
                    })
                    .unwrap_or(lp.planet_sma_au);

                // L3/L4/L5 are co-orbital with the planet (same heliocentric radius,
                // different phase angle).  A Hohmann gives 0 Delta-V in this case.
                // Use a phasing-orbit maneuver instead: lower into a shorter-period
                // orbit and drift the 60 deg (L4/L5) or 180 deg (L3) phase gap in N laps.
                let co_orbital = matches!(lp.point, 3 | 4 | 5)
                    && (r1_lp - lp.planet_sma_au).abs() < 0.02;

                if co_orbital {
                    let delta_phi = if lp.point == 3 {
                        std::f64::consts::PI           // L3: 180 deg opposition
                    } else {
                        std::f64::consts::FRAC_PI_3    // L4/L5: 60 deg
                    };
                    fleet_ui_state.computed_options =
                        co_orbital_phasing_options(lp.planet_sma_au, lp.gm, delta_phi);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    // Kinematic options: arc-length of the phase drift as proxy distance.
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(r1_lp);
                    let d = lp.planet_sma_au * delta_phi * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, 0.0, false
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
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
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
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                }
            }

        // ── Interstellar transfer computation ───────────────────────────────
        if let Some((_, _, distance_ly)) = star_system_snap {
            use crate::fleets::orbital_mechanics::{AU_IN_METERS, TransferOption};
            // 1 ly = 63 241.077 AU
            const AU_PER_LY: f64 = 63_241.077;
            let distance_m  = distance_ly as f64 * AU_PER_LY * AU_IN_METERS;
            let accel       = fleet.min_accel_ms2();
            let max_dv      = fleet.max_delta_v_ms();

            fleet_ui_state.computed_options.clear();

            let mut kinematics = kinematic_transfer_options(
                distance_m, accel, max_dv,
                0.0, 0.0, 0.0, true
            );
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
                        let is_co_orbital = matches!(lp.point, 3 | 4 | 5)
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
                                        egui::RichText::new(format!("{}", format_duration(window.time_to_window_s)))
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
                            .step_by(step_size as f64)
                            .custom_formatter(|v, _| {
                                if v < 0.01 {
                                    "Now".to_owned()
                                } else {
                                    format_duration(v as f64 * 86_400.0)
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
            ui.add_space(6.0);

            let fleet_max_dv = fleet.max_delta_v_ms();

            // Ensure selected_option is within bounds
            if fleet_ui_state.selected_option >= fleet_ui_state.computed_options.len() {
                fleet_ui_state.selected_option = fleet_ui_state.computed_options.len() - 1;
            }

            // Pre-compute execute button state
            let sel_option = fleet_ui_state.computed_options[fleet_ui_state.selected_option].clone();
            let abort_cost_t: f32 = if let Some(maneuver) = current_maneuver {
                let progress = maneuver.progress(elapsed) as f32;
                let abort_factor = 4.0 * progress * (1.0 - progress);
                maneuver.fuel_used_t * abort_factor * 0.6
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
            if is_interstellar {
                if let Some((_, ref sys_name, dist_ly)) = star_system_snap {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new(format!("\u{1F30C} Interstellar Mission: {}", sys_name))
                                .strong().size(13.0).color(theme::GRAVITY_ASSIST),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Distance: {:.2} ly = {:.0} AU",
                                dist_ly,
                                dist_ly as f64 * 63_241.077
                            )).size(11.0).color(theme::TEXT_DIM),
                        );
                        ui.label(
                            egui::RichText::new(
                                "\u{26A0} Interstellar navigation is point-and-burn. \
                                 Transfer windows do not apply. \
                                 Ensure adequate \u{394}V and life-support reserves."
                            ).size(11.0).italics().color(theme::AMBER),
                        );
                    });
                    ui.add_space(4.0);
                }
            }

            let btn_label = if is_interstellar {
                "\u{1F680} Commit Interstellar Course".to_string()
            } else if is_course_correction {
                if abort_cost_t > 0.01 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!("\u{1F504} Execute Course Correction (+{:.2} km/s abort burn)", abort_dv_kms)
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
                let insufficient = sel_option.is_thrust_limited && is_interstellar && sel_option.total_delta_v_ms == 0.0;
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
                                    build_planned_transfer(fleet_entity, fleet, orbit, fo.body, body_query, &sel_option, course_correction_sc)
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
                                        fleet_entity, fleet, orbit, flyby,
                                        body_query, &sel_option, course_correction_sc,
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
                                            let center_res = body_query.get(pt.orbit_center).ok().map(|(_, _, sc, _, _)| sc.position);
                                            let flyby_res  = body_query.get(flyby).ok().map(|(_, _, sc, _, _)| sc.position);
                                            let dest_res   = body_query.get(te).ok().map(|(_, _, sc, _, _)| sc.position);

                                            if let (Some(center_pos), Some(flyby_pos), Some(dest_pos)) =
                                                (center_res, flyby_res, dest_res)
                                            {

                                            let flyby_rel = flyby_pos - center_pos;
                                            let dest_rel  = dest_pos  - center_pos;
                                            let flyby_r   = flyby_rel.length();
                                            let dest_r    = dest_rel.length();

                                            let (.., leg2_sma, leg2_ecc) =
                                                hohmann_transfer(flyby_r, dest_r, GM_SUN);
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
                                            let leg2_mm = (GM_SUN / sma_m.powi(3)).sqrt();

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
                                    build_planned_transfer(fleet_entity, fleet, orbit, te, body_query, &sel_option, course_correction_sc)
                                }
                            } else {
                                build_planned_transfer(fleet_entity, fleet, orbit, te, body_query, &sel_option, course_correction_sc)
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
                if !is_interstellar {
                    let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                    let total_eta_s = dep_s + sel_option.transfer_time_s;
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("ETA  {}", format_duration(total_eta_s)))
                            .size(12.0)
                            .color(theme::GREEN),
                    );
                }
            });
            if !is_interstellar {
                let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                let total_eta_s = dep_s + sel_option.transfer_time_s;
                let arrival_ts = current_timestamp + total_eta_s as i64;
                ui.label(
                    egui::RichText::new(format!(
                        "Arrives  {}",
                        format_timestamp_date_time(arrival_ts)
                    ))
                    .size(11.0)
                    .color(theme::RP_BLUE),
                );
            }
            if !is_interstellar && !sel_affordable_with_abort {
                ui.label(
                    egui::RichText::new(
                        if abort_cost_t > 0.0 {
                            "Insufficient \u{394}V remaining after abort burn."
                        } else {
                            "Selected option requires more \u{394}V than this fleet can provide."
                        },
                    )
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
                let snapped: Vec<(usize, String, f64, f64, f64, f64)> =
                    fleet_ui_state.gravity_assist_candidates
                        .iter()
                        .enumerate()
                        .map(|(i, e)| (
                            i,
                            e.option.body_name.clone(),
                            e.option.dv_savings_ms,
                            e.option.extra_time_s,
                            e.option.window_period_s,
                            e.option.v_inf_ms,
                        ))
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
                                    egui::RichText::new(
                                        format!("{sign}{}", format_duration(extra_t.abs()))
                                    )
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
                                let label = if beneficial { "⚡ Use Gravity Assist" } else { "Use Suboptimal Assist" };
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

        if !fleet_ui_state.computed_options.is_empty() {
            let fleet_wet_mass = fleet.total_wet_mass_t();
            let fleet_max_dv = fleet.max_delta_v_ms();

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Transfer Options:").strong().size(13.0));
            ui.add_space(2.0);

            let options: Vec<_> = fleet_ui_state.computed_options.clone();
            for (idx, option) in options.iter().enumerate() {
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
                            option.label
                        ))
                        .size(13.0)
                        .strong()
                        .color(row_color),
                    );
                    if resp.clicked() {
                        fleet_ui_state.selected_option = idx;
                        fleet_ui_state.planned_transfer = None;
                    }

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

                            ui.label(egui::RichText::new("Est. fuel:").size(12.0));
                            let fuel_color = if affordable {
                                theme::AMBER
                            } else {
                                theme::RED
                            };
                            ui.label(
                                egui::RichText::new(format!("{:.0} t ({fuel_pct}%)", fuel_cost))
                                    .size(12.0)
                                    .color(fuel_color),
                            );
                            ui.label(egui::RichText::new("Departure burn:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.delta_v1_ms))
                                    .size(12.0),
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
                                let (profile_label, profile_color) =
                                    if option.is_thrust_limited {
                                        // Burn time >= Hohmann time: impulsive assumption invalid.
                                        ("⚠ Thrust-limited", theme::RED)
                                    } else if option.label == "Full Thrust" {
                                        // Entire trip is a burn
                                        ("⚡ Full thrust", theme::AMBER)
                                    } else {
                                        let ratio = option.burn_time_s / option.transfer_time_s.max(1.0);
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
                                        egui::RichText::new("  Low-thrust spiral — travel time ≥ burn time")
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
fn build_planned_transfer(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    target_entity: Entity,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    option: &TransferOption,
    // For course corrections: the fleet's actual current position (in whatever
    // frame matches the central-body coordinates, typically heliocentric AU).
    // When set, used instead of the origin body's position for orbital-element
    // derivation so the Keplerian arc starts from the fleet, not from Jupiter.
    course_correction_pos: Option<bevy::math::DVec3>,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::{AU_IN_METERS, G_CONST, GM_SUN};

    let (_, origin_body, origin_sc, origin_ko, origin_lp) = body_query.get(orbit.body).ok()?;
    let (_, dest_body, _dest_sc, dest_ko, dest_lp) = body_query.get(target_entity).ok()?;

    let dest_parent = dest_lp.map(|lp| lp.0);
    let origin_parent = origin_lp.map(|lp| lp.0);
    let dest_is_star = dest_body.body_type == BodyType::Star;
    let dest_is_ring = dest_body.body_type == BodyType::Ring;

    // Determine: (origin_sma, dest_sma, gm, orbit_center, actual destination body for FleetOrbit)
    // For Rings: redirect the FleetOrbit destination to the ring's parent planet.
    // For Stars: Fleet will orbit the star at the planet SOI boundary; orbit_center = star entity.
    let (origin_sma_au, dest_sma_au, gm, orbit_center, actual_dest_body) = if dest_is_star {
        // Heliocentric escape: orbit body = current body's parent star
        let parent_mass = origin_body.mass;
        let planet_sma_au = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0);
        let soi_au = planet_sma_au * (parent_mass / 1.989e30_f64).powf(0.4);
        (orbit.radius_au, soi_au.max(orbit.radius_au * 50.0), G_CONST * parent_mass, target_entity, target_entity)
    } else if dest_is_ring {
        // Ring: resolve to orbiting the ring's parent planet at ring.radius altitude
        let ring_parent = dest_parent.unwrap_or(orbit.body);
        let parent_mass = body_query.get(ring_parent).ok()
            .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
        let ring_radius_au = (dest_body.radius as f64 * 1_000.0) / AU_IN_METERS;
        let r1 = if ring_parent == orbit.body {
            orbit.radius_au
        } else {
            origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.01)
        };
        (r1, ring_radius_au, G_CONST * parent_mass, ring_parent, ring_parent)
    } else if dest_parent == Some(orbit.body) {
        // Local (e.g., Earth → Moon)
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        (orbit.radius_au, r2, G_CONST * origin_body.mass, orbit.body, target_entity)
    } else if dest_parent.is_some() && dest_parent == origin_parent {
        // Both orbit the same central body (moon-to-moon OR interplanetary, e.g. Earth→Mars).
        // NOTE: The Sun lacks SpaceCoordinates so body_query.get(Sun) fails — fall back to GM_SUN.
        let shared = dest_parent.unwrap();
        let gm = body_query.get(shared).ok()
            .map(|(_, b, _, _, _)| if b.body_type == BodyType::Star { GM_SUN } else { G_CONST * b.mass })
            .unwrap_or(GM_SUN);
        let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        (r1, r2, gm, shared, target_entity)
    } else if Some(target_entity) == origin_parent {
        // Downward transfer: fleet is at a moon, destination is the parent planet.
        // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
        let parent_mass = dest_body.mass;
        let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        let r2 = (dest_body.radius as f64 * 3_000.0) / AU_IN_METERS;
        (r1, r2.min(r1 * 0.5), G_CONST * parent_mass, target_entity, target_entity)
    } else {
        // Heliocentric: if fleet is at a moon, its own SMA is Earth-relative — use parent's SMA.
        let r1 = if origin_ko.map(|ko| ko.semi_major_axis < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
            origin_parent
                .and_then(|pe| body_query.get(pe).ok())
                .and_then(|(_, _, _, ko, _)| ko)
                .map(|ko| ko.semi_major_axis)
                .or_else(|| origin_ko.map(|ko| ko.semi_major_axis))
                .unwrap_or(1.0)
        } else {
            origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0)
        };
        let r2 = if dest_ko.map(|ko| ko.semi_major_axis < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
            dest_parent
                .and_then(|pe| body_query.get(pe).ok())
                .and_then(|(_, _, _, ko, _)| ko)
                .map(|ko| ko.semi_major_axis)
                .or_else(|| dest_ko.map(|ko| ko.semi_major_axis))
                .unwrap_or(1.5)
        } else {
            dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.5)
        };
        // Require the star to be close to the heliocentric origin (< 1 AU) so that nearby
        // star catalog entities with large SpaceCoordinates are not mistakenly selected.
        let star = body_query.iter()
            .find(|(_, b, sc, ko, _)| ko.is_none() && b.body_type == BodyType::Star && sc.position.length_squared() < 1.0)
            .map(|(e, _, _, _, _)| e)
            .unwrap_or(orbit.body);
        (r1, r2, GM_SUN, star, target_entity)
    };

    // For course corrections, determine outward/inward from the fleet's actual distance vs
    // the destination distance.  The body SMAs may not reflect the fleet's position mid-transit.
    // (Computed after rel_pos and dest_rel are available below; use a closure to defer.)
    let center_pos = body_query.get(orbit_center).map(|(_, _, sc, _, _)| sc.position).unwrap_or(bevy::math::DVec3::ZERO);
    // For course corrections use the fleet's actual position; otherwise use the origin body.
    let rel_pos = if let Some(fleet_pos) = course_correction_pos {
        // fleet_pos is already in the correct frame (heliocentric or planet-relative).
        // If the orbit center has coordinates, convert fleet_pos to center-relative.
        // cc_local_pos from the caller is already planet-relative for local transfers,
        // but heliocentric for Sun transfers — both are relative to the frame origin,
        // not the orbit_center entity.  Subtract center_pos for consistency.
        fleet_pos - center_pos
    } else {
        // For heliocentric transfers where the fleet orbits a moon, the moon's
        // SpaceCoordinates stores only a local offset from its parent planet — not a
        // heliocentric position.  Use the parent planet's heliocentric SC so that the
        // departure direction (argument_of_periapsis) points in the correct direction.
        let origin_helio_pos = if origin_body.body_type == BodyType::Moon && center_pos.length_squared() < 1e-20 {
            origin_parent
                .and_then(|pe| body_query.get(pe).ok())
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        };
        origin_helio_pos - center_pos
    };

    // Derive the transfer-orbit plane from the 3D departure and arrival position
    // vectors relative to the central body (r1 × r2 gives the plane normal).
    // This keeps inclination, LAN, and argument_of_periapsis mutually consistent
    // so the propagated green-dot position and the displayed preview arc match.
    // For heliocentric transfers where the destination is a moon, its SpaceCoordinates
    // also stores only a local offset — use the parent planet's position instead.
    let dest_sc_pos = body_query.get(target_entity).ok()
        .map(|(_, b, sc, _, lp)| {
            if b.body_type == BodyType::Moon && center_pos.length_squared() < 1e-20 {
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
        let dest_r  = dest_rel.length();
        dest_r >= fleet_r
    } else {
        dest_sma_au >= origin_sma_au
    };

    let plane_normal = rel_pos.cross(dest_rel);
    let plane_normal_len = plane_normal.length();

    let (transfer_inclination, transfer_lan, argument_of_periapsis) = if plane_normal_len > 1e-20 {
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
            if outward { omega } else { omega + std::f64::consts::PI }
        } else {
            let departure_angle = rel_pos.y.atan2(rel_pos.x);
            if outward { departure_angle } else { departure_angle - std::f64::consts::PI }
        };
        (incl, lan, aop)
    } else {
        // Degenerate (origin and destination collinear with center): ecliptic-flat.
        let departure_angle = rel_pos.y.atan2(rel_pos.x);
        let aop = if outward { departure_angle } else { departure_angle - std::f64::consts::PI };
        (0.0, 0.0, aop)
    };

    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
    let sma_m = option.sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: option.sma_au,
        eccentricity: option.eccentricity,
        inclination: transfer_inclination,
        longitude_ascending_node: transfer_lan,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    // Arrival orbit radius: for rings use the ring radius, otherwise reuse fleet parking radius
    let arrival_orbit_radius_au = if dest_is_ring {
        dest_sma_au
    } else if dest_is_star {
        dest_sma_au // park at SOI boundary initially
    } else {
        orbit.radius_au
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body: actual_dest_body,
        orbit_center,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label: option.label,
        start_position_au: None,
        end_position_au: None,
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
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::AU_IN_METERS;

    // LP transfers are heliocentric – find the star as orbit center.
    // Use the proximity guard to skip distant nearby-star catalog entities.
    let star_entity = body_query.iter()
        .find(|(_, b, sc, ko, _)| ko.is_none() && b.body_type == BodyType::Star && sc.position.length_squared() < 1.0)
        .map(|(e, _, _, _, _)| e)
        .unwrap_or(orbit.body);

    // Determine departure position.  For fleets orbiting the star directly
    // (e.g. after a previous LP transfer), `orbit.body` is the star whose
    // SpaceCoordinates are at the heliocentric origin → rel_pos would be
    // (0,0,0) and departure_angle 0.  In that case use the L-point's parent
    // planet position instead so the orbit geometry is meaningful.
    let center_pos = body_query.get(star_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(bevy::math::DVec3::ZERO);

    let origin_pos = {
        let (_, body_data, origin_sc, _, _) = body_query.get(orbit.body).ok()?;
        if body_data.body_type == BodyType::Star {
            // Fleet is parked around the star — use the planet's current position
            // as the departure reference instead.
            body_query.get(lp.planet_entity)
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
        "Moderate"  => "Direct Moderate",
        "Fast"      => "Direct Fast",
        other       => other, // kinematic labels (Full Thrust, Coast, Max Speed, Direct *) pass through
    };

    // Pre-compute the heliocentric LP position for kinematic arc rendering.
    // Every LP transfer sets start/end positions so the fleet flies to the correct
    // Lagrange-point location rather than the star origin (0,0,0).
    let planet_pos = body_query.get(lp.planet_entity)
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
    let lp_pos_au = center_pos + bevy::math::DVec3::new(
        lp.radius_au * lp_angle.cos(),
        lp.radius_au * lp_angle.sin(),
        0.0,
    );
    let start_pos = Some(origin_pos);
    let end_pos   = Some(lp_pos_au);

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
        orbit_center: star_entity,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label,
        start_position_au: start_pos,
        end_position_au: end_pos,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    })
}
