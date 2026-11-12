use perfboard_core::{
    ConstraintKind, ConstraintSeverity, Diagnostic, DiagnosticCode, DiagnosticSeverity,
    ObjectReference,
};

use crate::Design;

pub(crate) fn validate_structure(design: &Design) -> Result<(), String> {
    for constraint in design.constraints.values() {
        let component = |id| design.components.contains_key(&id);
        match &constraint.kind {
            ConstraintKind::FixedPlacement { component_id, .. }
            | ConstraintKind::Orientation { component_id, .. }
            | ConstraintKind::Access { component_id }
            | ConstraintKind::EdgePlacement { component_id, .. }
                if !component(*component_id) =>
            {
                return Err("constraint refers to a missing component".into());
            }
            ConstraintKind::Proximity {
                first_component_id,
                second_component_id,
                max_distance_holes,
            } => {
                if !component(*first_component_id) || !component(*second_component_id) {
                    return Err("proximity constraint refers to a missing component".into());
                }
                if !max_distance_holes.is_finite() || *max_distance_holes < 0.0 {
                    return Err("proximity distance must be finite and non-negative".into());
                }
            }
            ConstraintKind::PreferredMaximumLength {
                net_id,
                max_length_mm,
            } => {
                if !design.nets.contains_key(net_id)
                    || !max_length_mm.is_finite()
                    || *max_length_mm <= 0.0
                {
                    return Err("maximum length constraint is invalid".into());
                }
            }
            ConstraintKind::ReservedArea { area, .. } => {
                if area.min.x > area.max.x
                    || area.min.y > area.max.y
                    || area.max.x >= design.board.size.holes_x()
                    || area.max.y >= design.board.size.holes_y()
                {
                    return Err("constraint area is outside the board".into());
                }
            }
            ConstraintKind::AssemblyAccess { component_id, area } => {
                if !component(*component_id)
                    || area.min.x > area.max.x
                    || area.min.y > area.max.y
                    || area.max.x >= design.board.size.holes_x()
                    || area.max.y >= design.board.size.holes_y()
                {
                    return Err("assembly-access constraint is invalid".into());
                }
            }
            ConstraintKind::WireSide { net_id, .. } if !design.nets.contains_key(net_id) => {
                return Err("wire-side constraint refers to a missing net".into());
            }
            ConstraintKind::LockedRoute { wire_id } if !design.wires.contains_key(wire_id) => {
                return Err("locked-route constraint refers to a missing wire".into());
            }
            _ => {}
        }
    }
    for item in design.jumper_inventory.values() {
        if item.id.trim().is_empty()
            || item.color.trim().is_empty()
            || item
                .wire_diameter_mm
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || item
                .body_height_mm
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err("jumper inventory entry is invalid".into());
        }
    }
    for choice in design.crossing_choices.values() {
        if choice.first_wire_id == choice.second_wire_id
            || ![choice.first_wire_id, choice.second_wire_id].contains(&choice.over_wire_id)
            || !design.wires.contains_key(&choice.first_wire_id)
            || !design.wires.contains_key(&choice.second_wire_id)
        {
            return Err("crossing choice is invalid".into());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // The exhaustive typed-constraint match is easier to audit in one place.
pub(crate) fn diagnostics(design: &Design) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for constraint in design.constraints.values() {
        let violated = match &constraint.kind {
            ConstraintKind::FixedPlacement {
                component_id,
                position,
            } => design.components[component_id].position != *position,
            ConstraintKind::Orientation {
                component_id,
                rotation,
            } => design.components[component_id].rotation != *rotation,
            ConstraintKind::Proximity {
                first_component_id,
                second_component_id,
                max_distance_holes,
            } => {
                let first = design.components[first_component_id].position;
                let second = design.components[second_component_id].position;
                let dx = f64::from(first.x.abs_diff(second.x));
                let dy = f64::from(first.y.abs_diff(second.y));
                dx.hypot(dy) > *max_distance_holes
            }
            ConstraintKind::EdgePlacement {
                component_id,
                max_edge_distance_holes,
            } => {
                let point = design.components[component_id].position;
                let distance = point
                    .x
                    .min(point.y)
                    .min(design.board.size.intervals_x() - point.x)
                    .min(design.board.size.intervals_y() - point.y);
                distance > *max_edge_distance_holes
            }
            ConstraintKind::WireSide { net_id, side } => design
                .wires
                .values()
                .any(|wire| wire.net_id == *net_id && wire.side != *side),
            ConstraintKind::LockedRoute { wire_id } => !design.wires[wire_id].locked,
            ConstraintKind::PreferredMaximumLength {
                net_id,
                max_length_mm,
            } => design.wires.values().any(|wire| {
                wire.net_id == *net_id
                    && perfboard_core::measure_route(&wire.vertices, design.board.pitch_mm)
                        .length_mm
                        > *max_length_mm
            }),
            ConstraintKind::BendSpacing {
                min_half_grid_steps,
            } => design.wires.values().any(|wire| {
                wire.vertices.windows(4).any(|points| {
                    let spacing = (points[2].x2 - points[1].x2)
                        .abs()
                        .max((points[2].y2 - points[1].y2).abs());
                    u32::try_from(spacing).unwrap_or(u32::MAX) < *min_half_grid_steps
                })
            }),
            ConstraintKind::ReservedArea { area, .. } => {
                design.components.values().any(|component| {
                    component.position.x >= area.min.x
                        && component.position.x <= area.max.x
                        && component.position.y >= area.min.y
                        && component.position.y <= area.max.y
                }) || design
                    .wires
                    .values()
                    .any(|wire| route_enters_area(&wire.vertices, area))
            }
            ConstraintKind::AssemblyAccess { component_id, area } => {
                design.components.values().any(|component| {
                    component.id != *component_id
                        && component.position.x >= area.min.x
                        && component.position.x <= area.max.x
                        && component.position.y >= area.min.y
                        && component.position.y <= area.max.y
                }) || design
                    .wires
                    .values()
                    .any(|wire| route_enters_area(&wire.vertices, area))
            }
            ConstraintKind::Access { .. } => false,
        };
        if violated {
            result.push(Diagnostic {
                code: DiagnosticCode(format!(
                    "constraint.{}",
                    match constraint.severity {
                        ConstraintSeverity::Required => "required",
                        ConstraintSeverity::Preferred => "preferred",
                        ConstraintSeverity::Advisory => "advisory",
                    }
                )),
                severity: match constraint.severity {
                    ConstraintSeverity::Required => DiagnosticSeverity::Error,
                    ConstraintSeverity::Preferred => DiagnosticSeverity::Warning,
                    ConstraintSeverity::Advisory => DiagnosticSeverity::Advisory,
                },
                message: if constraint.rationale.is_empty() {
                    "Constraint is not satisfied".into()
                } else {
                    constraint.rationale.clone()
                },
                objects: vec![ObjectReference {
                    kind: "constraint".into(),
                    id: constraint.id.to_string(),
                }],
            });
        }
    }
    result
}

#[allow(clippy::similar_names)] // Rectangle bounds are clearest as parallel x/y names.
fn route_enters_area(
    vertices: &[perfboard_core::RoutePos],
    area: &perfboard_core::GridRect,
) -> bool {
    let min_x2 = i64::from(area.min.x) * 2;
    let max_x2 = i64::from(area.max.x) * 2;
    let min_y2 = i64::from(area.min.y) * 2;
    let max_y2 = i64::from(area.max.y) * 2;
    vertices.windows(2).any(|pair| {
        let (dx, dy, steps) = crate::connectivity::segment_step(pair);
        if steps == 0 {
            return false;
        }
        (0..=steps).any(|step| {
            let x2 = pair[0].x2 + dx * step;
            let y2 = pair[0].y2 + dy * step;
            (min_x2..=max_x2).contains(&x2) && (min_y2..=max_y2).contains(&y2)
        })
    })
}
