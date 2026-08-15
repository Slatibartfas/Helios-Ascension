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

    if ty == "ResearchSpeed" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Research speed +{}%", v as i64),
        ));
    }
    if ty == "EngineeringSpeed" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Engineering speed +{}%", v as i64),
        ));
    }

    if ty == "PopulationGrowth" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Population growth +{:.1}%/yr", v / 100.0),
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
        return Some((
            EffectTone::Positive,
            format!("Stockpile capacity +{:.0}% (all resources)", v * 100.0),
        ));
    }

    if ty == "NitrogenHarvesting" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Harvests {} Mt/yr N\u{2082}", format_mining_rate(v)),
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
