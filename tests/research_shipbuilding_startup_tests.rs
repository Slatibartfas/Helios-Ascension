use bevy::prelude::*;

use helios_ascension::economy::{GlobalBudget, PendingResourceRequests};
use helios_ascension::research::{ResearchPlugin, ResearchState, TechnologiesData};
use helios_ascension::shipbuilding::{ShipbuildingData, ShipbuildingPlugin};
use helios_ascension::ui::SimulationTime;

#[test]
#[ignore = "flaky on main since GRA-40 (#104): cargo_module not completed. \
           TODO: file child issue + fix (see GRA-39 final-lap comment)."]
fn startup_completes_baseline_ship_component_engineering() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(GlobalBudget::default())
        .insert_resource(PendingResourceRequests::default())
        .insert_resource(SimulationTime::new())
        .add_plugins(ResearchPlugin)
        .add_plugins(ShipbuildingPlugin);

    app.update();

    let research_state = app.world().resource::<ResearchState>();
    let shipbuilding_data = app.world().resource::<ShipbuildingData>();
    let technologies_data = app.world().resource::<TechnologiesData>();

    assert!(
        !shipbuilding_data.modules.is_empty(),
        "ship modules should load at startup"
    );
    assert!(
        technologies_data
            .get_component("probe_avionics_core")
            .is_some(),
        "merged ship module engineering catalog should expose probe avionics"
    );
    assert!(
        research_state.is_component_completed("probe_avionics_core"),
        "baseline computing should complete probe avionics engineering"
    );
    assert!(
        research_state.is_component_completed("solar_panel_array"),
        "startup solar power should complete solar array engineering"
    );
    assert!(
        research_state.is_component_completed("cargo_module"),
        "baseline space technology should complete cargo module engineering"
    );
    assert!(
        research_state.is_component_completed("survey_radar_suite"),
        "baseline sensors should complete survey radar engineering"
    );
}
