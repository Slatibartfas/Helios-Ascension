//! GRA-367-B Phase 2: snapshot tests for the unified selected-option card.
//!
//! One golden per transfer class locks the `CardWidget` schema so
//! Phase-3/4/5/6 children can extend the data without silently
//! breaking the player-facing layout.  Goldens are hand-rolled
//! `.txt` files under `tests/golden/transfer_card_*.txt` (no `insta`
//! dependency — see `_default/gra-367-b/PLAN.md` for the rationale).

use bevy::math::DVec3;
use bevy::prelude::Entity;
use helios_ascension::fleets::components::{TransferPlan, TransferReferenceFrame};
use helios_ascension::fleets::orbital_mechanics::{GravityAssistOption, TransferOption};
use helios_ascension::fleets::porkchop::{PorkchopCell, PorkchopGrid, PorkchopMetric};
use helios_ascension::ui::transfer_planner_card::{
    build_selected_card, frame_caption, CardSupplement, CardWidget, FleetInfo,
};
use helios_ascension::ui::GravityAssistEntry;
use std::fs;
use std::path::PathBuf;

/// Flat-text renderer so the snapshot harness doesn't need an egui
/// context (and the goldens stay readable in PR review).
fn render_to_string(card: &CardWidget) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n", card.title));
    if let Some(sub) = &card.subtitle {
        out.push_str(&format!("  {sub}\n"));
    }
    out.push('\n');
    for row in &card.rows {
        out.push_str(&format!("  {:<24} {}\n", row.label, row.value));
    }
    if !card.legs.is_empty() {
        out.push('\n');
        for leg in &card.legs {
            out.push_str(&format!("  [{}] {}\n", leg.leg_label, leg.summary));
        }
    }
    if let Some(frame) = &card.frame_caption {
        out.push('\n');
        out.push_str(&format!("  frame: {frame}\n"));
    }
    if let Some(warn) = &card.warn {
        out.push('\n');
        out.push_str(&format!("  WARN: {warn}\n"));
    }
    out
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("transfer_card_{name}.txt"))
}

/// Compare against the on-disk golden.  When `UPDATE_GOLDENS=1` is
/// set, write the rendered text instead of comparing — useful for
/// the first PR when the goldens don't yet exist.
fn assert_golden(name: &str, card: &CardWidget) {
    let rendered = render_to_string(card);
    let path = golden_path(name);
    if std::env::var("UPDATE_GOLDENS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create golden dir");
        }
        fs::write(&path, &rendered).expect("write golden");
        return;
    }
    let golden = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}\n--- rendered ---\n{rendered}\n--- run with UPDATE_GOLDENS=1 to create",
            path.display()
        )
    });
    assert_eq!(
        rendered.trim(),
        golden.trim(),
        "card snapshot drifted — run with UPDATE_GOLDENS=1 to refresh"
    );
}

/// Toy Tsiolkovsky rocket-equation closure so snapshot tests don't
/// need a real `Fleet` resource.  m_dry = 1 t, ve = 2500 m/s.
fn linear_fuel_cost(dv_ms: f64) -> f64 {
    let ve = 2_500.0_f64;
    let m_dry = 1.0_f64;
    m_dry * ((dv_ms / ve).exp() - 1.0)
}

fn sample_porkchop_grid() -> PorkchopGrid {
    PorkchopGrid {
        origin_name: "Earth".to_string(),
        dest_name: "Mars".to_string(),
        t_dep_bounds_s: (0.0, 60.0 * 86_400.0),
        tof_bounds_s: (180.0 * 86_400.0, 260.0 * 86_400.0),
        rendered_tof_bounds_s: (180.0 * 86_400.0, 260.0 * 86_400.0),
        resolution: (3, 1),
        cells: vec![PorkchopCell {
            t_dep_s: 30.0 * 86_400.0,
            tof_s: 220.0 * 86_400.0,
            total_dv_ms: 5_600.0,
            c3_departure: 9.0e6,
            v_inf_arrival_ms: 2_500.0,
            delta_v1_ms: 3_600.0,
            delta_v2_ms: 2_000.0,
            feasible: true,
            origin_pos_au: DVec3::new(1.0, 0.0, 0.0),
            dest_pos_au: DVec3::new(1.5, 0.0, 0.0),
            v_departure_ms: DVec3::new(30_000.0, 0.0, 0.0),
            v_arrival_ms: DVec3::new(24_000.0, 0.0, 0.0),
            transfer_orbit: None,
        }],
        min_cell: Some((0, 0)),
        metric: PorkchopMetric::TotalDv,
    }
}

fn sample_transfer_option() -> TransferOption {
    TransferOption {
        label: "Moderate",
        total_delta_v_ms: 940.0,
        delta_v1_ms: 620.0,
        delta_v2_ms: 320.0,
        transfer_time_s: 3.5 * 86_400.0,
        sma_au: 0.0,
        eccentricity: 0.0,
        energy_multiplier: 1.05,
        burn_time_s: 720.0,
        plane_change_dv_ms: 0.0,
        is_thrust_limited: false,
        transfer_orbit_override: None,
    }
}

fn sample_fleet_info() -> FleetInfo {
    FleetInfo {
        max_delta_v_ms: 6_000.0,
        wet_mass_t: 850.0,
    }
}

#[test]
fn snapshot_porkchop_card() {
    let plan = TransferPlan {
        porkchop_grid: Some(sample_porkchop_grid()),
        selected_porkchop_cell: Some((0, 0)),
        ..TransferPlan::default()
    };
    let card = build_selected_card(&plan, None, sample_fleet_info(), linear_fuel_cost);
    assert_golden("porkchop", &card);
}

#[test]
fn snapshot_three_option_card() {
    let plan = TransferPlan {
        computed_options: vec![sample_transfer_option()],
        selected_option: 0,
        ..TransferPlan::default()
    };
    let card = build_selected_card(&plan, None, sample_fleet_info(), linear_fuel_cost);
    assert_golden("three_option", &card);
}

#[test]
fn snapshot_interstellar_card() {
    let plan = TransferPlan::default();
    let sup = CardSupplement {
        star_system_snap: Some((1, "Alpha Centauri".to_string(), 4.37)),
        ..CardSupplement::default()
    };
    let card = build_selected_card(&plan, Some(&sup), sample_fleet_info(), linear_fuel_cost);
    assert_golden("interstellar", &card);
}

#[test]
fn snapshot_cross_star_card() {
    let tof_min = 100.0 * 365.25 * 86_400.0;
    let tof_max = 120.0 * 365.25 * 86_400.0;
    let grid = PorkchopGrid {
        origin_name: "Sol".to_string(),
        dest_name: "Proxima".to_string(),
        t_dep_bounds_s: (0.0, 0.0),
        tof_bounds_s: (tof_min, tof_max),
        rendered_tof_bounds_s: (tof_min, tof_max),
        resolution: (1, 1),
        cells: vec![PorkchopCell {
            t_dep_s: 0.0,
            tof_s: tof_max,
            total_dv_ms: 35_000.0,
            c3_departure: 0.0,
            v_inf_arrival_ms: 0.0,
            delta_v1_ms: 17_500.0,
            delta_v2_ms: 17_500.0,
            feasible: true,
            origin_pos_au: DVec3::ZERO,
            dest_pos_au: DVec3::ZERO,
            v_departure_ms: DVec3::ZERO,
            v_arrival_ms: DVec3::ZERO,
            transfer_orbit: None,
        }],
        min_cell: Some((0, 0)),
        metric: PorkchopMetric::TotalDv,
    };
    let plan = TransferPlan::default();
    let sup = CardSupplement {
        cross_system_grid: Some(grid),
        cross_system_selected: Some((0, 0)),
        ..CardSupplement::default()
    };
    let card = build_selected_card(&plan, Some(&sup), sample_fleet_info(), linear_fuel_cost);
    assert_golden("cross_star", &card);
}

#[test]
fn snapshot_gravity_assist_card() {
    let ga = GravityAssistOption {
        body_name: "Venus".to_string(),
        flyby_radius_au: 0.72,
        v_inf_ms: 5_000.0,
        max_dv_assist_ms: 1_500.0,
        total_dv_ms: 4_400.0,
        dv_savings_ms: 1_200.0,
        total_time_s: 280.0 * 86_400.0,
        extra_time_s: 90.0 * 86_400.0,
        window_period_s: 584.0 * 86_400.0,
        leg1_time_s: 90.0 * 86_400.0,
        leg2_time_s: 190.0 * 86_400.0,
        dv_depart_ms: 1_200.0,
        dv_mid_ms: 0.0,
        dv_arrive_ms: 3_200.0,
    };
    let entry = GravityAssistEntry {
        option: ga,
        flyby_entity: Entity::from_raw_u32(7).expect("valid entity index"),
    };
    let plan = TransferPlan::default();
    let sup = CardSupplement {
        gravity_assist_candidates: vec![entry],
        ..CardSupplement::default()
    };
    let card = build_selected_card(&plan, Some(&sup), sample_fleet_info(), linear_fuel_cost);
    assert_golden("gravity_assist", &card);
}

#[test]
fn frame_caption_supports_both_variants() {
    // Phase 1 read-only indicator uses these two values; Phase 6 wires
    // the override.  Document the public mapping so the indicator stays
    // consistent across phases.
    let bary = frame_caption(Some(TransferReferenceFrame::SystemBarycentric))
        .expect("barycentric caption");
    assert!(bary.contains("Barycentric"), "{bary}");
    let body = frame_caption(Some(TransferReferenceFrame::Body(
        Entity::from_raw_u32(1).expect("valid entity index"),
    )))
    .expect("body caption");
    assert!(body.contains("Body"), "{body}");
    assert!(frame_caption(None).is_none());
}

#[test]
fn no_selection_yields_empty_placeholder() {
    let plan = TransferPlan::default();
    let card = build_selected_card(&plan, None, sample_fleet_info(), linear_fuel_cost);
    assert!(card.title.contains("no transfer selected"));
    assert!(card.rows.is_empty());
}
