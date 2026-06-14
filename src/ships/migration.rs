//! Migration shim for legacy `ShipClass::Freighter` entities (GRA-40).
//!
//! Before this issue every freighter carried the legacy
//! `ShipInfo.class == ShipClass::Freighter` enum tag; the cargo capacity
//! came from the `freighter_frame` hull plus whatever `cargo_pod_medium`
//! modules the player fitted.  This shim adds a [`ShipTemplateRef`] +
//! per-slot [`ShipSlot`] to every such entity, mapping 1:1 onto the
//! `light_freighter` template (which matches today's cargo capacity).
//!
//! The shim runs as a `Startup` system.  New ships created post-merge get
//! the components assigned at construction time (see
//! `shipbuilding::systems::process_ship_launches_and_completions`), so this
//! shim only ever touches entities from pre-GRA-40 saves or hand-spawned
//! test fixtures.

use bevy::prelude::*;

use crate::fleets::{ShipClass, ShipInstance};
use crate::research::ResearchState;

use super::components::{FreighterSlots, FreighterTemplateMarker, ShipSlot, ShipTemplateRef};
use super::templates::{FreighterTemplateRegistry, LEGACY_MIGRATION_TEMPLATE_ID};

/// Add `ShipTemplateRef` + per-slot `ShipSlot` + `FreighterTemplateMarker`
/// to any `ShipInstance` whose `info.class == ShipClass::Freighter` and
/// that does not already carry a `ShipTemplateRef`.
///
/// Idempotent: re-running on an already-migrated world is a no-op because
/// the `Without<ShipTemplateRef>` filter excludes the migrated entities.
pub fn migrate_legacy_freighters(
    mut commands: Commands,
    registry: Res<FreighterTemplateRegistry>,
    freighters: Query<
        Entity,
        (
            With<ShipInstance>,
            Without<ShipTemplateRef>,
            Without<FreighterTemplateMarker>,
        ),
    >,
    ships: Query<&ShipInstance>,
) {
    if registry.is_empty() {
        // Registry failed to load or no templates defined — skip migration
        // so we don't assign an unknown id.  Other code paths that query
        // the registry will see it empty and degrade gracefully.
        return;
    }

    let Some(light) = registry.get(LEGACY_MIGRATION_TEMPLATE_ID) else {
        warn!(
            "Freighter template '{}' not found in registry; skipping legacy \
             freighter migration ({} legacy entities unaffected)",
            LEGACY_MIGRATION_TEMPLATE_ID,
            freighters.iter().count()
        );
        return;
    };

    let mut migrated = 0u32;
    for entity in &freighters {
        let Ok(ship) = ships.get(entity) else {
            continue;
        };
        if ship.info.class != ShipClass::Freighter {
            // Entity passed the `Without<FreighterTemplateMarker>` filter
            // but the inner class isn't Freighter — shouldn't happen in
            // normal play, but skip safely if it does.
            continue;
        }

        commands.entity(entity).insert((
            ShipTemplateRef::new(LEGACY_MIGRATION_TEMPLATE_ID),
            FreighterTemplateMarker,
            FreighterSlots::new(
                light
                    .cargo_slots
                    .iter()
                    .map(|slot| ShipSlot::new(&slot.hull_slot_id, &slot.default_module, 0))
                    .collect(),
            ),
        ));
        migrated += 1;
    }

    if migrated > 0 {
        info!(
            "Migrated {} legacy ShipClass::Freighter entities to template '{}'",
            migrated, LEGACY_MIGRATION_TEMPLATE_ID
        );
    }
}

/// Helper used by the construction completion path to derive a freighter
/// template id from a hull id.  Used when a new ship is built from a hull
/// that exactly matches a freighter template's base_hull — the player
/// defaults to that template's `light_freighter` (or the cheapest
/// era-tier-1 template for that hull).
///
/// Returns `None` for non-freighter classes or when the hull has no
/// matching template.  Callers should fall back to the legacy
/// `ShipClass`-based path in that case.
pub fn default_template_for_hull(
    registry: &FreighterTemplateRegistry,
    ship_class: ShipClass,
    hull_id: &str,
    research_state: &ResearchState,
) -> Option<String> {
    if ship_class != ShipClass::Freighter {
        return None;
    }

    // Find all templates whose base_hull matches and whose required_tech is
    // researched.  Among those, pick the cheapest-era one — that's the
    // LGD's "the default is the cheapest era tier whose hull matches"
    // rule.  If none have researched tech, fall back to the cheapest-era
    // template regardless of tech (so the ship can be queued even if
    // research is not yet done; the shipyard path gates research on its
    // own).
    let mut matches: Vec<&super::templates::FreighterTemplate> = registry
        .iter()
        .filter_map(|(_, t)| {
            if t.base_hull != hull_id {
                return None;
            }
            if let Some(ref tech) = t.required_tech {
                if !research_state.is_unlocked(tech) {
                    return None;
                }
            }
            Some(t)
        })
        .collect();

    if matches.is_empty() {
        matches = registry
            .iter()
            .filter_map(|(_, t)| {
                if t.base_hull == hull_id {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();
    }

    matches
        .into_iter()
        .min_by_key(|t| t.era_tier)
        .map(|t| t.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::PropulsionType;
    use crate::fleets::ShipInfo;

    fn make_freighter_template() -> super::super::templates::FreighterTemplate {
        super::super::templates::FreighterTemplate {
            id: "light_freighter".to_string(),
            display_name: "Light".to_string(),
            description: String::new(),
            base_hull: "freighter_frame".to_string(),
            era_tier: 1,
            required_tech: None,
            cargo_slots: vec![super::super::templates::CargoSlot {
                hull_slot_id: "cargo_a".to_string(),
                default_module: "cargo_pod_medium".to_string(),
                upgrade_path: vec![],
            }],
            tags: vec![],
        }
    }

    #[test]
    fn migration_adds_template_ref_to_legacy_freighter() {
        let mut world = World::new();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(make_freighter_template());
        world.insert_resource(registry);
        world.insert_resource(ResearchState::default());

        let info = ShipInfo {
            name: "Legacy Freighter".to_string(),
            hull_id: None,
            class: ShipClass::Freighter,
            dry_mass_t: 1.0,
            fuel_mass_t: 0.0,
            max_fuel_t: 0.0,
            thrust_kn: 0.0,
            isp_s: 0.0,
            propulsion: PropulsionType::Chemical,
            cargo_capacity_t: 0.0,
        };
        let entity = world
            .spawn(ShipInstance::new(
                info,
                Entity::PLACEHOLDER,
                0.0,
                false,
                None,
                0,
            ))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(migrate_legacy_freighters);
        schedule.run(&mut world);

        assert!(world.get::<ShipTemplateRef>(entity).is_some());
        let slots_comp = world
            .get::<super::super::components::FreighterSlots>(entity)
            .expect("FreighterSlots added");
        let slot = slots_comp
            .0
            .iter()
            .find(|s| s.slot_id == "cargo_a")
            .expect("cargo_a slot added");
        assert_eq!(slot.installed_module, "cargo_pod_medium");
        assert_eq!(slot.upgrade_tier, 0);
    }

    #[test]
    fn migration_skips_non_freighter_ships() {
        let mut world = World::new();
        let mut registry = FreighterTemplateRegistry::default();
        registry.insert(make_freighter_template());
        world.insert_resource(registry);
        world.insert_resource(ResearchState::default());

        let info = ShipInfo {
            name: "Frigate".to_string(),
            hull_id: None,
            class: ShipClass::Frigate,
            dry_mass_t: 1.0,
            fuel_mass_t: 0.0,
            max_fuel_t: 0.0,
            thrust_kn: 0.0,
            isp_s: 0.0,
            propulsion: PropulsionType::Chemical,
            cargo_capacity_t: 0.0,
        };
        let entity = world
            .spawn(ShipInstance::new(
                info,
                Entity::PLACEHOLDER,
                0.0,
                false,
                None,
                0,
            ))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(migrate_legacy_freighters);
        schedule.run(&mut world);

        assert!(world.get::<ShipTemplateRef>(entity).is_none());
        assert!(world
            .get::<super::super::components::FreighterSlots>(entity)
            .is_none());
    }
}
