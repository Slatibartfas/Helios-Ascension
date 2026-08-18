//! Pure-data helpers for building / construction card presentation.
//!
//! Moved from `src/ui/construction/data.rs` as part of Phase 1.5C of the
//! bevy_ui widget-extraction plan. These functions transform game data
//! (`BuildingModifierDef`, throughput numbers, headcounts) into
//! presentation-ready tuples — they have no bevy dependencies and belong
//! next to the colony data they describe.

use crate::colony::data::BuildingModifierDef;

/// Effect-bullet tone (drives the color of the corresponding line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectTone {
    Positive,
    Negative,
    Neutral,
    Cost,
    Throughput,
}

/// Format a megatonnes-per-year rate with the SI mass ladder.
/// Mirrors the format used by the build card's "production" effect lines.
pub fn format_mining_rate(mt_per_year: f64) -> String {
    if mt_per_year.abs() < 1e-12 {
        return "0".to_string();
    }
    let v = mt_per_year.abs();
    // SI ladder for **Mt** input. Boundaries land each suffix in
    // the 1..=999 range (one- to three-digit display).
    if v < 1e-9 {
        format!("{:.0} g/yr", mt_per_year * 1e12)
    } else if v < 1e-6 {
        format!("{:.0} kg/yr", mt_per_year * 1e9)
    } else if v < 1e-3 {
        format!("{:.0} t/yr", mt_per_year * 1e6)
    } else if v < 1.0 {
        format!("{:.0} kt/yr", mt_per_year * 1e3)
    } else if v < 1e3 {
        format!("{:.0} Mt/yr", mt_per_year)
    } else if v < 1e6 {
        format!("{:.0} Gt/yr", mt_per_year / 1e3)
    } else if v < 1e9 {
        format!("{:.2} Tt/yr", mt_per_year / 1e6)
    } else if v < 1e12 {
        format!("{:.2} Pt/yr", mt_per_year / 1e9)
    } else if v < 1e15 {
        format!("{:.2} Et/yr", mt_per_year / 1e12)
    } else if v < 1e18 {
        format!("{:.2} Zt/yr", mt_per_year / 1e15)
    } else {
        format!("{:.2} Yt/yr", mt_per_year / 1e18)
    }
}

/// Format a headcount value with the SI ladder (`K`, `M`, `B`, `T`).
pub fn format_residents(people: f64) -> String {
    let v = people.abs();
    if v < 1.0 {
        return format!("{:.0}", people);
    }
    if v < 1e3 {
        return format!("{:.0}", people);
    }
    if v < 1e6 {
        return format!("{:.1}k", people / 1e3);
    }
    if v < 1e9 {
        return format!("{:.2}M", people / 1e6);
    }
    if v < 1e12 {
        return format!("{:.2}B", people / 1e9);
    }
    format!("{:.2}T", people / 1e12)
}

/// Convert a building modifier to a (tone, display) pair for the build card.
///
/// Returns `None` for unrecognized modifier types — those are silently hidden
/// (they exist in the RON but are not surfaced on the card; add a case here
/// to surface a new type).
///
/// Recognized modifier types (v3.1 §0.H.2 + v3.8.12):
///   * `*Production` — Iron, Aluminum, ..., Water, Food
///   * `HousingCapacity`
///   * `HydrogenSynthesis`, `AmmoniaSynthesis`, `PolymerSynthesis` (ChemicalPlant)
///   * `ResearchSpeed`, `EngineeringSpeed`
///   * `PopulationGrowth`
///   * `WealthGeneration`
///   * `LogisticsCapacity`
///   * `StorageCapacity`
///   * `NitrogenHarvesting`
///   * `PlutoniumBreeding`, `TritiumBreeding`
///   * `ConstructionCost` (negative = "builds faster", positive = "more expensive")
///   * `BuildPointsProduction` (the Factory's actual effect: +10 BP/yr per build)
pub fn friendly_label(m: &BuildingModifierDef) -> Option<(EffectTone, String)> {
    let ty = m.modifier_type.as_str();
    let v = m.value;

    if ty == "BuildPointsProduction" && v > 0.0 {
        return Some((EffectTone::Positive, format!("Builds +{} BP/yr", v as i64)));
    }

    if let Some(res_name) = ty.strip_suffix("Production") {
        if v > 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Produces {} {}", format_mining_rate(v), res_name),
            ));
        }
        return None;
    }

    // v3.10 (GRA-22c Phase 4A): dedicated labels for the
    // post-Fab modifiers that don't fit the generic `<X>Production`
    // format. The Fab is now the planet's electronics-production
    // source; surface the absolute Mt/yr contribution with a
    // built-in unit ask so the player reads "Produces 1 Mt/yr".
    // Sub-inventory: consumers of `ElectronicsProduction` are
    // pinned to a future "Electronics" economic resource
    // (logging + data-centres + spacecraft avionics). The label
    // here is cosmetic until the resource is wired up in a later
    // phase.
    if ty == "ElectronicsProduction" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Produces {:.2} Mt/yr electronics", v),
        ));
    }

    if let Some(elem) = ty.strip_suffix("Breeding") {
        if v > 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Breeds {} {}", format_mining_rate(v), elem),
            ));
        }
        return None;
    }

    if let Some(elem) = ty.strip_suffix("Synthesis") {
        if v > 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Synthesizes {} {}", format_mining_rate(v), elem),
            ));
        }
        return None;
    }

    // v3.10 (GRA-22c Phase 4A): the RON value is **RP/month per build**
    // (see `src/research/systems.rs::update_research_points`, which
    // divides `value` by `SECONDS_PER_YEAR / 12` to land in RP/sec).
    // The earlier `+100%` label was a regression — it conflated the
    // building's contribution with the percent-multiplier
    // `ResearchSpeed` modifier tracked by `ResearchState` (a separate
    // path that scales ALL research). The two paths today share a
    // string key but different units; the canary label surfaces the
    // building's absolute RP/month contribution. 100 RP/month per
    // ResearchLab matches the Phase 1.7 calibration.
    if ty == "ResearchSpeed" && v > 0.0 {
        return Some((EffectTone::Positive, format!("+{:.0} RP/month", v)));
    }
    if ty == "EngineeringSpeed" && v > 0.0 {
        return Some((EffectTone::Positive, format!("+{:.0} EP/month", v)));
    }

    if ty == "PopulationGrowth" && v > 0.0 {
        // v3.9 (GRA-22c Phase 1.3): the RON value is a raw fraction per
        // build per year (e.g. 0.0003 = +0.030%/yr per center). This
        // matches the `Colony::population_growth_per_year` reading path
        // verbatim — the previously-displayed "+0.5%/yr" was a 16.7×
        // over-statement (RON was basis-points, not a percent).
        return Some((
            EffectTone::Positive,
            format!("Population growth +{:.3}%/yr", v * 100.0),
        ));
    }

    if ty == "WealthGeneration" && v > 0.0 {
        return Some((EffectTone::Positive, format!("Generates {:.0} MC/yr", v)));
    }

    if ty == "LogisticsCapacity" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Logistics capacity {:.0} t/yr", v),
        ));
    }

    if ty == "StorageCapacity" && v > 0.0 {
        // v3.10 (GRA-22c Phase 4B): the modifier value is now
        // an absolute Mt amount per resource per depot
        // (additive), not a percent. The canary label reads
        // "+5,000 Mt to stockpile caps (all resources)" rather
        // than the legacy "+25%". The actor's reading is the
        // same shape as the LifeSupport scrubber output:
        // absolute Mt contribution, not a percent.
        return Some((
            EffectTone::Positive,
            format!("+{:.0} Mt to stockpile caps (all resources)", v),
        ));
    }

    if ty == "NitrogenHarvesting" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Harvests {} Mt/yr N\u{2082}", format_mining_rate(v)),
        ));
    }

    // v3.10 (GRA-22c Phase 4B): LifeSupport scrubber output.
    // Distinct from the generic `*Production` because the player
    // sees "scrubs" rather than "produces" — CO₂ is a
    // waste-stream to remove, not a product to stock. The label
    // reads "Scrubs 30 Mt/yr CO₂ (exportable)" so the player knows
    // the CO₂ is recoverable (Terra Invicta-style atmospheric
    // export).
    if ty == "CarbonDioxideScrubbing" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Scrubs {:.0} Mt/yr CO\u{2082} (exportable)", v),
        ));
    }

    if ty == "HousingCapacity" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Houses {} residents", format_residents(v)),
        ));
    }

    if ty == "ConstructionCost" {
        if v < 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Builds {} BP/yr faster", (-v) as i64),
            ));
        } else if v > 0.0 {
            return Some((
                EffectTone::Neutral,
                format!("Construction cost +{} BP/build", v as i64),
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::data::BuildingModifierDef;

    fn make_mod(ty: &str, value: f64) -> BuildingModifierDef {
        BuildingModifierDef {
            modifier_type: ty.to_string(),
            value,
        }
    }

    #[test]
    fn friendly_label_production() {
        let m = make_mod("IronProduction", 5.0);
        let (tone, label) = friendly_label(&m).unwrap();
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Iron"));
    }

    #[test]
    fn friendly_label_unknown_returns_none() {
        let m = make_mod("UnknownModifier", 1.0);
        assert!(friendly_label(&m).is_none());
    }

    #[test]
    fn format_mining_rate_zero() {
        assert_eq!(format_mining_rate(0.0), "0");
    }

    #[test]
    fn format_mining_rate_megatonnes() {
        let s = format_mining_rate(50.0);
        assert!(s.contains("Mt/yr"));
    }

    #[test]
    fn format_residents_thousands() {
        assert_eq!(format_residents(2_500.0), "2.5k");
    }

    #[test]
    fn format_residents_millions() {
        let s = format_residents(1_500_000.0);
        assert!(s.contains("M"));
    }
}
