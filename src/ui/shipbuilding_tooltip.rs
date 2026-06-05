use super::dashboard::format_mass_compact;
use crate::economy::ResourceType;
use crate::shipbuilding::{HullSlotDefinition, ShipModuleDefinition};

#[derive(Clone, Copy)]
pub(super) enum ShipbuildingTooltipTone {
    Neutral,
    Positive,
    Warning,
    Negative,
    Accent,
    Muted,
}

pub(super) enum ShipbuildingTooltipEntry {
    Paragraph(String),
    Stat {
        label: String,
        value: String,
        tone: ShipbuildingTooltipTone,
    },
    Spacer,
}

pub(super) struct ShipbuildingTooltipContent {
    pub title: String,
    pub entries: Vec<ShipbuildingTooltipEntry>,
}

pub(super) fn build_module_tooltip(
    module: &ShipModuleDefinition,
    slot: Option<&HullSlotDefinition>,
) -> ShipbuildingTooltipContent {
    let mut entries = Vec::new();

    if !module.description.is_empty() {
        entries.push(ShipbuildingTooltipEntry::Paragraph(
            module.description.clone(),
        ));
        entries.push(ShipbuildingTooltipEntry::Spacer);
    }

    push_stat(
        &mut entries,
        "Category",
        module.category.display_name().to_string(),
        ShipbuildingTooltipTone::Accent,
    );
    push_stat(
        &mut entries,
        "Size",
        module.size.clone(),
        ShipbuildingTooltipTone::Neutral,
    );

    if let Some(slot) = slot {
        push_stat(
            &mut entries,
            "Slot",
            format!(
                "{} ({})",
                prettify_slot_name(&slot.slot_id),
                if slot.required {
                    "required"
                } else {
                    "optional"
                }
            ),
            ShipbuildingTooltipTone::Muted,
        );
    }

    entries.push(ShipbuildingTooltipEntry::Spacer);
    entries.extend(module_stat_lines(module));

    ShipbuildingTooltipContent {
        title: module.display_name.clone(),
        entries,
    }
}

pub(super) fn build_slot_tooltip(
    slot: &HullSlotDefinition,
    installed_module: Option<&ShipModuleDefinition>,
    compatible_modules: &[&ShipModuleDefinition],
) -> ShipbuildingTooltipContent {
    let mut entries = Vec::new();

    push_stat(
        &mut entries,
        "Category",
        slot.category.display_name().to_string(),
        ShipbuildingTooltipTone::Accent,
    );
    push_stat(
        &mut entries,
        "Size",
        slot.size.clone(),
        ShipbuildingTooltipTone::Neutral,
    );
    push_stat(
        &mut entries,
        "Socket",
        if slot.required {
            "Required".to_string()
        } else {
            "Optional".to_string()
        },
        if slot.required {
            ShipbuildingTooltipTone::Warning
        } else {
            ShipbuildingTooltipTone::Muted
        },
    );

    if let Some(rotation_deg) = slot.rotation_deg {
        push_stat(
            &mut entries,
            "Arc Rotation",
            format!("{} deg", format_number(rotation_deg as f64)),
            ShipbuildingTooltipTone::Neutral,
        );
    }

    push_stat(
        &mut entries,
        "Current Fit",
        installed_module
            .map(|module| module.display_name.clone())
            .unwrap_or_else(|| "Empty".to_string()),
        if installed_module.is_some() {
            ShipbuildingTooltipTone::Positive
        } else {
            ShipbuildingTooltipTone::Muted
        },
    );
    push_stat(
        &mut entries,
        "Unlocked Fits",
        compatible_modules.len().to_string(),
        ShipbuildingTooltipTone::Neutral,
    );

    if let Some(module) = installed_module {
        entries.push(ShipbuildingTooltipEntry::Spacer);
        entries.extend(module_stat_lines(module));
    } else if !compatible_modules.is_empty() {
        let suggestions = compatible_modules
            .iter()
            .take(3)
            .map(|module| module.display_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        push_stat(
            &mut entries,
            "Suggested Fits",
            suggestions,
            ShipbuildingTooltipTone::Accent,
        );
    }

    ShipbuildingTooltipContent {
        title: prettify_slot_name(&slot.slot_id),
        entries,
    }
}

pub(super) fn prettify_slot_name(slot_id: &str) -> String {
    slot_id
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = first.to_uppercase().collect::<String>();
                    word.push_str(chars.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn module_stat_lines(module: &ShipModuleDefinition) -> Vec<ShipbuildingTooltipEntry> {
    let mut lines = Vec::new();

    push_stat(
        &mut lines,
        "Mass",
        format!("{} t", format_number(module.dry_mass_t)),
        ShipbuildingTooltipTone::Neutral,
    );
    push_stat(
        &mut lines,
        "Build Points",
        format!("{} BP", format_number(module.build_points)),
        ShipbuildingTooltipTone::Neutral,
    );

    if module.power_generation_mw > 0.0 {
        push_stat(
            &mut lines,
            "Power Generation",
            format!("+{} MW", format_number(module.power_generation_mw)),
            ShipbuildingTooltipTone::Positive,
        );
    }
    if module.power_draw_mw > 0.0 {
        push_stat(
            &mut lines,
            "Power Draw",
            format!("-{} MW", format_number(module.power_draw_mw)),
            ShipbuildingTooltipTone::Warning,
        );
    }
    if module.power_generation_mw > 0.0 || module.power_draw_mw > 0.0 {
        push_stat(
            &mut lines,
            "Net Power",
            format!(
                "{} MW",
                format_signed_number(module.power_generation_mw - module.power_draw_mw)
            ),
            if module.power_generation_mw - module.power_draw_mw >= 0.0 {
                ShipbuildingTooltipTone::Positive
            } else {
                ShipbuildingTooltipTone::Negative
            },
        );
    }
    if module.thrust_kn > 0.0 {
        push_stat(
            &mut lines,
            "Thrust",
            format!("{} kN", format_number(module.thrust_kn)),
            ShipbuildingTooltipTone::Accent,
        );
    }
    if module.isp_s > 0.0 {
        push_stat(
            &mut lines,
            "Specific Impulse",
            format!("{} s", format_number(module.isp_s)),
            ShipbuildingTooltipTone::Accent,
        );
    }
    if let Some(propulsion) = module.propulsion {
        push_stat(
            &mut lines,
            "Propulsion",
            propulsion.display_name().to_string(),
            ShipbuildingTooltipTone::Accent,
        );
    }
    if module.construction_capacity_bp_per_year > 0.0 {
        push_stat(
            &mut lines,
            "Construction Capacity",
            format!(
                "+{} BP/year",
                format_number(module.construction_capacity_bp_per_year)
            ),
            ShipbuildingTooltipTone::Positive,
        );
    }
    if module.launch_capacity_t_per_year > 0.0 {
        push_stat(
            &mut lines,
            "Launch Capacity",
            format!(
                "+{} t/year",
                format_number(module.launch_capacity_t_per_year)
            ),
            ShipbuildingTooltipTone::Positive,
        );
    }

    for (name, value) in &module.attribute_values {
        if let Some((label, formatted, tone)) = format_attribute(name, *value) {
            push_stat(&mut lines, &label, formatted, tone);
        }
    }

    if !module.resource_costs.is_empty() {
        push_stat(
            &mut lines,
            "Materials",
            format_shipbuilding_resource_costs_inline(&module.resource_costs, 4),
            ShipbuildingTooltipTone::Muted,
        );
    }

    if let Some(required_tech) = &module.required_tech {
        push_stat(
            &mut lines,
            "Tech Requirement",
            title_case(required_tech),
            ShipbuildingTooltipTone::Warning,
        );
    }
    push_stat(
        &mut lines,
        "Engineering Project",
        title_case(module.engineering_project_id()),
        ShipbuildingTooltipTone::Warning,
    );

    lines
}

fn format_attribute(name: &str, value: f64) -> Option<(String, String, ShipbuildingTooltipTone)> {
    if value.abs() <= f64::EPSILON {
        return None;
    }

    match name {
        "crew" | "crew_capacity" => Some((
            "Crew Capacity".to_string(),
            format!("+{}", format_number(value)),
            ShipbuildingTooltipTone::Neutral,
        )),
        "fuel_capacity_t" => Some((
            "Fuel Storage".to_string(),
            format!("+{} t", format_number(value)),
            ShipbuildingTooltipTone::Positive,
        )),
        "cargo_capacity_t" => Some((
            "Cargo Storage".to_string(),
            format!("+{} t", format_number(value)),
            ShipbuildingTooltipTone::Positive,
        )),
        "ordnance_capacity_t" => Some((
            "Ordnance Payload".to_string(),
            format!("+{} t", format_number(value)),
            ShipbuildingTooltipTone::Accent,
        )),
        "magazine_capacity_t" => Some((
            "Magazine Capacity".to_string(),
            format!("+{} t", format_number(value)),
            ShipbuildingTooltipTone::Accent,
        )),
        "sensor_range_au" => Some((
            "Sensor Range".to_string(),
            format!("+{} AU", format_number(value)),
            ShipbuildingTooltipTone::Accent,
        )),
        "sensor_range_km" => Some((
            "Sensor Range".to_string(),
            format!("+{} km", format_number(value)),
            ShipbuildingTooltipTone::Accent,
        )),
        "docking_ports" => Some((
            "Docking Ports".to_string(),
            format!("+{}", format_number(value)),
            ShipbuildingTooltipTone::Neutral,
        )),
        "isru_rate_t_per_year" => Some((
            "ISRU Rate".to_string(),
            format!("+{} t/year", format_number(value)),
            ShipbuildingTooltipTone::Positive,
        )),
        "heat_sink_capacity" => Some((
            "Heat Sink Capacity".to_string(),
            format!("+{}", format_number(value)),
            ShipbuildingTooltipTone::Warning,
        )),
        "maintenance_rate" => Some((
            "Maintenance Rate".to_string(),
            format!("+{}", format_number(value)),
            ShipbuildingTooltipTone::Warning,
        )),
        "orbital_build_slots" => Some((
            "Orbital Build Slots".to_string(),
            format!("+{}", format_number(value)),
            ShipbuildingTooltipTone::Positive,
        )),
        "flex_space" => Some((
            "Flexible Payload Space".to_string(),
            format!("+{} bay", format_number(value)),
            ShipbuildingTooltipTone::Accent,
        )),
        "mining_efficiency" => Some((
            "Mining Efficiency".to_string(),
            format!("+{}%", format_number(value)),
            ShipbuildingTooltipTone::Positive,
        )),
        "reveal_hidden" => Some((
            "Special".to_string(),
            "Reveals hidden contacts".to_string(),
            ShipbuildingTooltipTone::Accent,
        )),
        _ => Some((
            title_case(name),
            format_number(value),
            ShipbuildingTooltipTone::Neutral,
        )),
    }
}

fn push_stat(
    lines: &mut Vec<ShipbuildingTooltipEntry>,
    label: &str,
    value: String,
    tone: ShipbuildingTooltipTone,
) {
    lines.push(ShipbuildingTooltipEntry::Stat {
        label: label.to_string(),
        value,
        tone,
    });
}

pub(super) fn format_shipbuilding_resource_costs_inline(
    costs: &[(ResourceType, f64)],
    max_items: usize,
) -> String {
    let mut parts = Vec::new();
    for (index, (resource, amount)) in costs.iter().enumerate() {
        if index >= max_items {
            parts.push(format!("+{} more", costs.len() - max_items));
            break;
        }
        parts.push(format_shipbuilding_resource_cost(*resource, *amount));
    }
    parts.join(" | ")
}

pub(super) fn format_shipbuilding_resource_cost_lines(
    costs: &[(ResourceType, f64)],
    max_items: usize,
) -> Vec<String> {
    costs
        .iter()
        .take(max_items)
        .map(|(resource, amount)| format_shipbuilding_resource_cost(*resource, *amount))
        .collect()
}

pub(super) fn format_shipbuilding_resource_cost(resource: ResourceType, amount: f64) -> String {
    format!(
        "{} {}",
        resource.display_name(),
        format_mass_compact(amount)
    )
}

fn title_case(value: &str) -> String {
    value
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = first.to_uppercase().collect::<String>();
                    word.push_str(chars.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_number(value: f64) -> String {
    let precision = if value.abs() >= 100.0 {
        0
    } else if value.abs() >= 10.0 {
        1
    } else {
        2
    };
    let raw = match precision {
        0 => format!("{value:.0}"),
        1 => format!("{value:.1}"),
        _ => format!("{value:.2}"),
    };
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn format_signed_number(value: f64) -> String {
    if value >= 0.0 {
        format!("+{}", format_number(value))
    } else {
        format!("-{}", format_number(value.abs()))
    }
}
