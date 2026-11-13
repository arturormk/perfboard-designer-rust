use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BoardFeatures, BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId,
    ComponentKind, Diagnostic, DiagnosticCode, DiagnosticSeverity, LocalPos, ObjectReference,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementPolicy {
    Strict,
    AllowWarnings,
}

#[must_use]
#[allow(clippy::too_many_lines)] // One deterministic pass keeps cross-instance checks ordered.
pub fn validate_design_placements(
    board: BoardSize,
    features: &BoardFeatures,
    definitions: &BTreeMap<String, ComponentDefinition>,
    instances: &BTreeMap<ComponentInstanceId, ComponentInstance>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut occupied = BTreeMap::<(i64, i64), ComponentInstanceId>::new();
    let mut keepout_points = BTreeMap::<(i64, i64), ComponentInstanceId>::new();
    for (id, instance) in instances {
        let Some(definition) = definitions.get(&instance.definition_id) else {
            diagnostics.push(diagnostic(
                "component.definition_missing",
                DiagnosticSeverity::Error,
                format!(
                    "Definition `{}` is not embedded in this project",
                    instance.definition_id
                ),
                *id,
            ));
            continue;
        };
        if !definition.allowed_rotations.contains(&instance.rotation) {
            diagnostics.push(diagnostic(
                "component.rotation_not_allowed",
                DiagnosticSeverity::Error,
                format!("{} does not allow this rotation", definition.name),
                *id,
            ));
        }
        for local in definition.occupied_positions() {
            let point = transform(instance, local);
            if !inside(board, point) {
                diagnostics.push(diagnostic(
                    "placement.out_of_bounds",
                    DiagnosticSeverity::Error,
                    format!("{} occupies a hole outside the board", instance.name),
                    *id,
                ));
                continue;
            }
            if features.forbidden_areas.iter().any(|area| {
                point.0 >= i64::from(area.min.x)
                    && point.0 <= i64::from(area.max.x)
                    && point.1 >= i64::from(area.min.y)
                    && point.1 <= i64::from(area.max.y)
            }) {
                diagnostics.push(diagnostic(
                    "placement.forbidden_area",
                    DiagnosticSeverity::Error,
                    format!("{} overlaps a forbidden board area", instance.name),
                    *id,
                ));
            }
            if features.mounting_holes.iter().any(|hole| {
                point.0 == i64::from(hole.position.x) && point.1 == i64::from(hole.position.y)
            }) {
                diagnostics.push(diagnostic(
                    "placement.mounting_hole_occupied",
                    DiagnosticSeverity::Error,
                    format!("{} occupies a mounting hole", instance.name),
                    *id,
                ));
            }
            // A bridge is soldered onto existing pads after the other parts. Its
            // endpoints therefore neither reserve holes nor conflict with parts
            // already using those holes.
            if definition.kind != ComponentKind::Bridge
                && let Some(other) = occupied.insert(point, *id)
            {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode("placement.occupied_hole".into()),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "{} shares an occupied hole with another component",
                        instance.name
                    ),
                    objects: vec![object(*id), object(other)],
                });
            }
        }
        for keepout in &definition.keepouts {
            for x in keepout.min.x..=keepout.max.x {
                for y in keepout.min.y..=keepout.max.y {
                    let point = transform(instance, LocalPos::new(x, y));
                    if let Some(other) = keepout_points.insert(point, *id)
                        && other != *id
                    {
                        diagnostics.push(Diagnostic {
                            code: DiagnosticCode("placement.keepout_overlap".into()),
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "{} overlaps another component keepout",
                                instance.name
                            ),
                            objects: vec![object(*id), object(other)],
                        });
                    }
                }
            }
        }
        for access in &definition.access {
            if !access.required {
                continue;
            }
            let mut outside = false;
            for x in access.min.x..=access.max.x {
                for y in access.min.y..=access.max.y {
                    if !inside(board, transform(instance, LocalPos::new(x, y))) {
                        outside = true;
                    }
                }
            }
            if outside && !access.allow_board_overhang {
                diagnostics.push(diagnostic(
                    "placement.required_access_out_of_bounds",
                    DiagnosticSeverity::Error,
                    format!(
                        "Required access `{}` extends outside the board",
                        access.label
                    ),
                    *id,
                ));
            }
        }
    }

    let occupied_points: BTreeSet<_> = occupied.keys().copied().collect();
    for (point, owner) in &occupied {
        if let Some(keepout_owner) = keepout_points.get(point)
            && owner != keepout_owner
        {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode("placement.keepout_occupied".into()),
                severity: DiagnosticSeverity::Error,
                message: "A component occupies another component's keepout".into(),
                objects: vec![object(*owner), object(*keepout_owner)],
            });
        }
    }
    for (id, instance) in instances {
        let Some(definition) = definitions.get(&instance.definition_id) else {
            continue;
        };
        let own: BTreeSet<_> = definition
            .occupied_positions()
            .into_iter()
            .map(|point| transform(instance, point))
            .collect();
        for access in definition.access.iter().filter(|access| access.required) {
            let blocked = (access.min.x..=access.max.x).any(|x| {
                (access.min.y..=access.max.y).any(|y| {
                    let point = transform(instance, LocalPos::new(x, y));
                    (occupied_points.contains(&point) && !own.contains(&point))
                        || keepout_points.get(&point).is_some_and(|owner| owner != id)
                        || features.forbidden_areas.iter().any(|area| {
                            point.0 >= i64::from(area.min.x)
                                && point.0 <= i64::from(area.max.x)
                                && point.1 >= i64::from(area.min.y)
                                && point.1 <= i64::from(area.max.y)
                        })
                })
            });
            if blocked {
                diagnostics.push(diagnostic(
                    "placement.required_access_blocked",
                    DiagnosticSeverity::Error,
                    format!("Required access `{}` is blocked", access.label),
                    *id,
                ));
            }
        }
    }
    diagnostics.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then(left.message.cmp(&right.message))
    });
    diagnostics.dedup();
    diagnostics
}

fn transform(instance: &ComponentInstance, local: LocalPos) -> (i64, i64) {
    let (x, y) = instance.rotation.rotate_local((local.x, local.y));
    (
        i64::from(instance.position.x) + i64::from(x),
        i64::from(instance.position.y) + i64::from(y),
    )
}

fn inside(board: BoardSize, point: (i64, i64)) -> bool {
    point.0 >= 0
        && point.1 >= 0
        && point.0 < i64::from(board.holes_x())
        && point.1 < i64::from(board.holes_y())
}

fn object(id: ComponentInstanceId) -> ObjectReference {
    ObjectReference {
        kind: "component_instance".into(),
        id: id.to_string(),
    }
}

fn diagnostic(
    code: &str,
    severity: DiagnosticSeverity,
    message: String,
    id: ComponentInstanceId,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode(code.into()),
        severity,
        message,
        objects: vec![object(id)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HolePos, Rotation};
    use std::collections::BTreeMap;

    fn definition(source: &str) -> ComponentDefinition {
        ComponentDefinition::from_toml(source).unwrap()
    }

    fn instance(definition_id: &str, x: u32, y: u32) -> ComponentInstance {
        ComponentInstance {
            id: ComponentInstanceId::new(),
            definition_id: definition_id.into(),
            name: definition_id.into(),
            position: HolePos::new(x, y),
            rotation: Rotation::Deg0,
            properties: BTreeMap::new(),
            notes: String::new(),
            shunted: false,
        }
    }

    #[test]
    fn bridge_can_share_component_holes_and_keepouts() {
        let part = definition(
            "schema_version=1\nid='part'\ndefinition_version='1'\nname='Part'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=0\nwidth=5\nheight=2\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0\n[[keepouts]]\nmin={x=0,y=0}\nmax={x=1,y=0}",
        );
        let bridge = definition(
            "schema_version=1\nkind='bridge'\nid='bridge'\ndefinition_version='1'\nname='Bridge'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=-0.2\nwidth=2.54\nheight=0.4\n[[pins]]\nid='a'\nnumber='A'\nname='A'\nx=0\ny=0\n[[pins]]\nid='b'\nnumber='B'\nname='B'\nx=1\ny=0",
        );
        let part_instance = instance(&part.id, 1, 1);
        let bridge_instance = instance(&bridge.id, 1, 1);
        let definitions = BTreeMap::from([(part.id.clone(), part), (bridge.id.clone(), bridge)]);
        let instances = BTreeMap::from([
            (part_instance.id, part_instance),
            (bridge_instance.id, bridge_instance),
        ]);

        let diagnostics = validate_design_placements(
            BoardSize::new(4, 4).unwrap(),
            &BoardFeatures::default(),
            &definitions,
            &instances,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn bridge_endpoints_must_stay_on_the_board() {
        let bridge = definition(
            "schema_version=1\nkind='bridge'\nid='bridge'\ndefinition_version='1'\nname='Bridge'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=-0.2\nwidth=2.54\nheight=0.4\n[[pins]]\nid='a'\nnumber='A'\nname='A'\nx=0\ny=0\n[[pins]]\nid='b'\nnumber='B'\nname='B'\nx=1\ny=0",
        );
        let bridge_instance = instance(&bridge.id, 1, 0);
        let definitions = BTreeMap::from([(bridge.id.clone(), bridge)]);
        let instances = BTreeMap::from([(bridge_instance.id, bridge_instance)]);
        let diagnostics = validate_design_placements(
            BoardSize::new(2, 2).unwrap(),
            &BoardFeatures::default(),
            &definitions,
            &instances,
        );
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code.0 == "placement.out_of_bounds")
        );
    }
}
