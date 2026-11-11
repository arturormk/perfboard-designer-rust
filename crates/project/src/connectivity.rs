use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentKind, ConnectionState, Diagnostic, DiagnosticCode, DiagnosticSeverity,
    NetId, ObjectReference, PinRef, RoutePos, TerminalRef,
};

use crate::Design;

pub(crate) fn validate_structure(design: &Design) -> Result<(), String> {
    let mut assigned = BTreeMap::<PinRef, NetId>::new();
    for (net_id, net) in &design.nets {
        if net.name.trim().is_empty() {
            return Err("net name must not be blank".into());
        }
        for pin in &net.pins {
            pin_position(design, pin)
                .ok_or_else(|| format!("net {net_id} refers to a missing pin"))?;
            if let Some(other) = assigned.insert(pin.clone(), *net_id) {
                return Err(format!(
                    "pin is assigned to conflicting nets {other} and {net_id}"
                ));
            }
        }
    }
    validate_bridge_net_conflicts(design)?;
    for junction in design.junctions.values() {
        if !design.nets.contains_key(&junction.net_id) {
            return Err("junction refers to a missing net".into());
        }
        let sides: BTreeSet<_> = design
            .wires
            .values()
            .filter(|wire| {
                matches!(&wire.start, TerminalRef::Junction { junction_id } if *junction_id == junction.id)
                    || matches!(&wire.end, TerminalRef::Junction { junction_id } if *junction_id == junction.id)
            })
            .map(|wire| wire.side)
            .collect();
        if sides.len() > 1 && !junction.through_hole {
            return Err("opposite-side wires require an explicit through-hole junction".into());
        }
        if junction.position.x >= design.board.size.holes_x()
            || junction.position.y >= design.board.size.holes_y()
        {
            return Err("junction lies outside the board".into());
        }
    }
    for wire in design.wires.values() {
        if !design.nets.contains_key(&wire.net_id) {
            return Err("wire refers to a missing net".into());
        }
        if wire.start == wire.end {
            return Err("wire endpoints must be different".into());
        }
        if wire.vertices.len() < 2 {
            return Err("wire must contain at least two vertices".into());
        }
        terminal_position(design, wire.net_id, &wire.start)?;
        terminal_position(design, wire.net_id, &wire.end)?;
        for terminal in [&wire.start, &wire.end] {
            if let TerminalRef::RailNode { rail_id, .. } = terminal
                && design
                    .rails
                    .get(rail_id)
                    .is_some_and(|rail| rail.side != wire.side)
            {
                return Err("wire and connected rail must be on the same side".into());
            }
        }
        for vertex in &wire.vertices {
            if !route_inside(design.board.size, *vertex) {
                return Err("wire vertex lies outside the board".into());
            }
        }
        for pair in wire.vertices.windows(2) {
            let dx = (pair[1].x2 - pair[0].x2).abs();
            let dy = (pair[1].y2 - pair[0].y2).abs();
            if dx == 0 && dy == 0 {
                return Err("wire segments must have non-zero length".into());
            }
        }
    }
    for rail in design.rails.values() {
        if !design.nets.contains_key(&rail.net_id) {
            return Err("rail refers to a missing net".into());
        }
        if rail.vertices.len() < 2 {
            return Err("rail must contain at least two vertices".into());
        }
        for vertex in &rail.vertices {
            if !route_inside(design.board.size, *vertex) {
                return Err("rail vertex lies outside the board".into());
            }
        }
        if rail.vertices.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("rail segments must have non-zero length".into());
        }
    }
    Ok(())
}

fn validate_bridge_net_conflicts(design: &Design) -> Result<(), String> {
    for instance in design.components.values() {
        let definition = &design.component_definitions[&instance.definition_id];
        if definition.kind != ComponentKind::Bridge {
            continue;
        }
        let endpoint_positions: BTreeSet<_> = definition
            .pins
            .iter()
            .map(|pin| {
                let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
                RoutePos::new(
                    (i64::from(instance.position.x) + i64::from(x)) * 2,
                    (i64::from(instance.position.y) + i64::from(y)) * 2,
                )
            })
            .collect();
        let connected_nets: BTreeSet<_> = design
            .nets
            .iter()
            .filter_map(|(net_id, net)| {
                net.pins
                    .iter()
                    .any(|pin| {
                        pin_position(design, pin)
                            .is_some_and(|position| endpoint_positions.contains(&position))
                    })
                    .then_some(*net_id)
            })
            .collect();
        if connected_nets.len() > 1 {
            return Err("bridge connects pins assigned to conflicting nets".into());
        }
    }
    Ok(())
}

pub(crate) fn diagnostics(design: &Design) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for net in design.nets.values() {
        if net.state == ConnectionState::Proposed {
            result.push(diag(
                "circuit.proposed_net",
                DiagnosticSeverity::Advisory,
                format!("Net `{}` is a proposal awaiting approval", net.name),
                "net",
                net.id.to_string(),
            ));
            continue;
        }
        if net.pins.len() < 2 {
            result.push(diag(
                "circuit.net_too_small",
                DiagnosticSeverity::Warning,
                format!("Net `{}` has fewer than two pins", net.name),
                "net",
                net.id.to_string(),
            ));
            continue;
        }
        let adjacency = adjacency(design, net.id);
        let first = TerminalRef::from(net.pins.iter().next().expect("checked non-empty").clone());
        let mut reached = BTreeSet::new();
        let mut pending = vec![first];
        while let Some(current) = pending.pop() {
            if !reached.insert(current.clone()) {
                continue;
            }
            if let Some(next) = adjacency.get(&current) {
                pending.extend(next.iter().cloned());
            }
        }
        let complete = net
            .pins
            .iter()
            .all(|pin| reached.contains(&TerminalRef::from(pin.clone())));
        if !complete {
            result.push(diag(
                "circuit.net_incomplete",
                DiagnosticSeverity::Warning,
                format!("Net `{}` is not completely routed", net.name),
                "net",
                net.id.to_string(),
            ));
        }
    }
    for wire in design.wires.values() {
        let endpoints_match = terminal_position(design, wire.net_id, &wire.start)
            .ok()
            .zip(terminal_position(design, wire.net_id, &wire.end).ok())
            .is_some_and(|(start, end)| {
                wire.vertices.first() == Some(&start) && wire.vertices.last() == Some(&end)
            });
        if !endpoints_match {
            result.push(diag(
                "route.endpoint_moved",
                DiagnosticSeverity::Warning,
                "Wire endpoint no longer matches its terminal; edit or reroute it".into(),
                "wire",
                wire.id.to_string(),
            ));
        }
        if wire.side == perfboard_core::Side::Component
            && route_crosses_unrelated_component(design, wire)
        {
            result.push(diag(
                "route.component_clearance",
                DiagnosticSeverity::Warning,
                "Component-side wire passes through a component wire keepout".into(),
                "wire",
                wire.id.to_string(),
            ));
        }
        if has_short_interior_segment(&wire.vertices) {
            result.push(diag(
                "route.short_segment",
                DiagnosticSeverity::Warning,
                "Wire contains a segment shorter than one grid interval".into(),
                "wire",
                wire.id.to_string(),
            ));
        }
    }
    result.sort_by(|a, b| a.code.cmp(&b.code).then(a.message.cmp(&b.message)));
    result
}

fn has_short_interior_segment(vertices: &[RoutePos]) -> bool {
    vertices.windows(2).enumerate().any(|(index, pair)| {
        let is_endpoint_segment = index == 0 || index + 2 == vertices.len();
        let dx = (pair[1].x2 - pair[0].x2).abs();
        let dy = (pair[1].y2 - pair[0].y2).abs();
        !is_endpoint_segment && dx.max(dy) < 2
    })
}

fn route_crosses_unrelated_component(design: &Design, wire: &perfboard_core::Wire) -> bool {
    let endpoint_components: BTreeSet<_> = [&wire.start, &wire.end]
        .into_iter()
        .filter_map(|terminal| match terminal {
            TerminalRef::Pin { component_id, .. } => Some(*component_id),
            TerminalRef::Junction { .. } | TerminalRef::RailNode { .. } => None,
        })
        .collect();
    let blocked: BTreeSet<_> = design
        .components
        .values()
        .filter(|instance| !endpoint_components.contains(&instance.id))
        .flat_map(|instance| {
            let definition = &design.component_definitions[&instance.definition_id];
            definition
                .wire_keepouts
                .iter()
                .flat_map(|area| {
                    (i64::from(area.min.x) * 2..=i64::from(area.max.x) * 2).flat_map(move |x2| {
                        (i64::from(area.min.y) * 2..=i64::from(area.max.y) * 2)
                            .map(move |y2| (x2, y2))
                    })
                })
                .map(|(x2, y2)| {
                    let (x2, y2) = rotate_half_grid(instance.rotation, x2, y2);
                    (
                        i64::from(instance.position.x) * 2 + x2,
                        i64::from(instance.position.y) * 2 + y2,
                    )
                })
        })
        .collect();
    wire.vertices.windows(2).any(|pair| {
        let (dx, dy, steps) = segment_step(pair);
        (1..steps).any(|step| blocked.contains(&(pair[0].x2 + dx * step, pair[0].y2 + dy * step)))
    })
}

const fn rotate_half_grid(rotation: perfboard_core::Rotation, x2: i64, y2: i64) -> (i64, i64) {
    match rotation {
        perfboard_core::Rotation::Deg0 => (x2, y2),
        perfboard_core::Rotation::Deg90 => (-y2, x2),
        perfboard_core::Rotation::Deg180 => (-x2, -y2),
        perfboard_core::Rotation::Deg270 => (y2, -x2),
    }
}

fn adjacency(design: &Design, net_id: NetId) -> BTreeMap<TerminalRef, BTreeSet<TerminalRef>> {
    let mut map = BTreeMap::new();
    for wire in design.wires.values().filter(|wire| wire.net_id == net_id) {
        map.entry(wire.start.clone())
            .or_insert_with(BTreeSet::new)
            .insert(wire.end.clone());
        map.entry(wire.end.clone())
            .or_insert_with(BTreeSet::new)
            .insert(wire.start.clone());
    }
    for rail in design.rails.values().filter(|rail| rail.net_id == net_id) {
        let nodes = design
            .wires
            .values()
            .filter(|wire| wire.net_id == net_id)
            .flat_map(|wire| [&wire.start, &wire.end])
            .filter(|terminal| matches!(terminal, TerminalRef::RailNode { rail_id, .. } if *rail_id == rail.id))
            .cloned()
            .collect::<Vec<_>>();
        for (index, first) in nodes.iter().enumerate() {
            for second in nodes.iter().skip(index + 1) {
                add_connection(&mut map, first, second);
            }
        }
    }
    let terminals = design
        .nets
        .get(&net_id)
        .into_iter()
        .flat_map(|net| &net.pins)
        .filter_map(|pin| {
            pin_position(design, pin).map(|position| (TerminalRef::from(pin.clone()), position))
        })
        .collect::<Vec<_>>();
    for (index, (first, first_position)) in terminals.iter().enumerate() {
        for (second, second_position) in terminals.iter().skip(index + 1) {
            if first_position == second_position {
                add_connection(&mut map, first, second);
            }
        }
    }
    for instance in design.components.values() {
        let definition = &design.component_definitions[&instance.definition_id];
        if definition.kind != ComponentKind::Bridge {
            continue;
        }
        let endpoints = definition
            .pins
            .iter()
            .map(|pin| {
                let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
                RoutePos::new(
                    (i64::from(instance.position.x) + i64::from(x)) * 2,
                    (i64::from(instance.position.y) + i64::from(y)) * 2,
                )
            })
            .collect::<Vec<_>>();
        let Some((first_position, second_position)) = endpoints.first().zip(endpoints.get(1))
        else {
            continue;
        };
        let first = terminals
            .iter()
            .filter(|(_, position)| position == first_position);
        let second = terminals
            .iter()
            .filter(|(_, position)| position == second_position)
            .collect::<Vec<_>>();
        for (first, _) in first {
            for (second, _) in &second {
                add_connection(&mut map, first, second);
            }
        }
    }
    map
}

fn add_connection(
    map: &mut BTreeMap<TerminalRef, BTreeSet<TerminalRef>>,
    first: &TerminalRef,
    second: &TerminalRef,
) {
    map.entry(first.clone()).or_default().insert(second.clone());
    map.entry(second.clone()).or_default().insert(first.clone());
}

pub(crate) fn pin_position(design: &Design, pin: &PinRef) -> Option<RoutePos> {
    let instance = design.components.get(&pin.component_id)?;
    let definition = design.component_definitions.get(&instance.definition_id)?;
    let pin = definition
        .pins
        .iter()
        .find(|candidate| candidate.id == pin.pin_id)?;
    let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
    Some(RoutePos::new(
        (i64::from(instance.position.x) + i64::from(x)) * 2,
        (i64::from(instance.position.y) + i64::from(y)) * 2,
    ))
}

pub(crate) fn route_crosses_unrelated_pin(design: &Design, wire: &perfboard_core::Wire) -> bool {
    let endpoints: BTreeSet<_> = [&wire.start, &wire.end]
        .into_iter()
        .filter_map(|terminal| terminal_position(design, wire.net_id, terminal).ok())
        .collect();
    let pins: BTreeSet<_> = design
        .components
        .values()
        .flat_map(|instance| {
            let definition = &design.component_definitions[&instance.definition_id];
            definition.pins.iter().map(move |pin| {
                let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
                RoutePos::new(
                    (i64::from(instance.position.x) + i64::from(x)) * 2,
                    (i64::from(instance.position.y) + i64::from(y)) * 2,
                )
            })
        })
        .collect();
    wire.vertices.windows(2).any(|pair| {
        let (dx, dy, steps) = segment_step(pair);
        (0..=steps).any(|step| {
            let point = RoutePos::new(pair[0].x2 + dx * step, pair[0].y2 + dy * step);
            !endpoints.contains(&point) && pins.contains(&point)
        })
    })
}

pub(crate) fn rail_crosses_pin(design: &Design, rail: &perfboard_core::Rail) -> bool {
    let pins: BTreeSet<_> = design
        .components
        .values()
        .flat_map(|instance| {
            let definition = &design.component_definitions[&instance.definition_id];
            definition.pins.iter().map(move |pin| {
                let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
                RoutePos::new(
                    (i64::from(instance.position.x) + i64::from(x)) * 2,
                    (i64::from(instance.position.y) + i64::from(y)) * 2,
                )
            })
        })
        .collect();
    rail.vertices.windows(2).any(|pair| {
        let (dx, dy, steps) = segment_step(pair);
        (0..=steps).any(|step| {
            pins.contains(&RoutePos::new(
                pair[0].x2 + dx * step,
                pair[0].y2 + dy * step,
            ))
        })
    })
}

pub(crate) fn route_is_supported(vertices: &[RoutePos]) -> bool {
    vertices.len() >= 2 && vertices.windows(2).all(|pair| pair[0] != pair[1])
}

fn segment_step(pair: &[RoutePos]) -> (i64, i64, i64) {
    let total_dx = pair[1].x2 - pair[0].x2;
    let total_dy = pair[1].y2 - pair[0].y2;
    let steps = gcd(total_dx.unsigned_abs(), total_dy.unsigned_abs()) as i64;
    if steps == 0 {
        (0, 0, 0)
    } else {
        (total_dx / steps, total_dy / steps, steps)
    }
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

pub(crate) fn terminal_position(
    design: &Design,
    net_id: NetId,
    terminal: &TerminalRef,
) -> Result<RoutePos, String> {
    match terminal {
        TerminalRef::Pin {
            component_id,
            pin_id,
        } => {
            let pin = PinRef {
                component_id: *component_id,
                pin_id: pin_id.clone(),
            };
            let net = design.nets.get(&net_id).ok_or("missing net")?;
            if !net.pins.contains(&pin) {
                return Err("wire pin terminal is not assigned to its net".into());
            }
            pin_position(design, &pin).ok_or_else(|| "wire refers to a missing pin".into())
        }
        TerminalRef::Junction { junction_id } => {
            let junction = design
                .junctions
                .get(junction_id)
                .ok_or("wire refers to a missing junction")?;
            if junction.net_id != net_id {
                return Err("wire junction belongs to a different net".into());
            }
            Ok(junction.position.as_route_pos())
        }
        TerminalRef::RailNode { rail_id, position } => {
            let rail = design
                .rails
                .get(rail_id)
                .ok_or("wire refers to a missing rail")?;
            if rail.net_id != net_id {
                return Err("wire rail belongs to a different net".into());
            }
            if !rail.vertices.contains(position) {
                return Err("wire rail connection is not an explicit rail node".into());
            }
            Ok(*position)
        }
    }
}

fn route_inside(board: BoardSize, point: RoutePos) -> bool {
    point.x2 >= 0
        && point.y2 >= 0
        && point.x2 <= i64::from(board.intervals_x()) * 2
        && point.y2 <= i64::from(board.intervals_y()) * 2
}

fn diag(
    code: &str,
    severity: DiagnosticSeverity,
    message: String,
    kind: &str,
    id: String,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode(code.into()),
        severity,
        message,
        objects: vec![ObjectReference {
            kind: kind.into(),
            id,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Board, ProjectDocument};
    use perfboard_core::{
        BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ElectricalNet,
        HolePos, ProjectId, Rotation, Side, Wire, WireColor, WireId,
    };

    fn bridge_fixture() -> (Design, ComponentInstanceId, ComponentInstanceId) {
        let part = ComponentDefinition::from_toml(
            "schema_version=1\nid='part'\ndefinition_version='1'\nname='Part'\nallowed_rotations=['deg0']\n[body]\nx=-0.4\ny=-0.4\nwidth=0.8\nheight=0.8\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
        )
        .unwrap();
        let bridge = ComponentDefinition::from_toml(
            "schema_version=1\nkind='bridge'\nid='bridge'\ndefinition_version='1'\nname='Bridge'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=-0.2\nwidth=2.54\nheight=0.4\n[[pins]]\nid='a'\nnumber='A'\nname='A'\nx=0\ny=0\n[[pins]]\nid='b'\nnumber='B'\nname='B'\nx=1\ny=0",
        )
        .unwrap();
        let first_id = ComponentInstanceId::new();
        let second_id = ComponentInstanceId::new();
        let bridge_id = ComponentInstanceId::new();
        let mut document = ProjectDocument::new(
            ProjectId::new(),
            "bridge",
            Board::new("board", BoardSize::new(3, 2).unwrap(), 2.54),
        );
        document
            .design
            .component_definitions
            .insert(part.id.clone(), part.clone());
        document
            .design
            .component_definitions
            .insert(bridge.id.clone(), bridge.clone());
        for (id, definition_id, x) in [
            (first_id, part.id.as_str(), 0),
            (second_id, part.id.as_str(), 1),
            (bridge_id, bridge.id.as_str(), 0),
        ] {
            document.design.components.insert(
                id,
                ComponentInstance {
                    id,
                    definition_id: definition_id.into(),
                    name: definition_id.into(),
                    position: HolePos::new(x, 0),
                    rotation: Rotation::Deg0,
                    properties: BTreeMap::new(),
                    notes: String::new(),
                },
            );
        }
        (document.design, first_id, second_id)
    }

    #[test]
    fn supported_routes_are_non_zero_polylines() {
        assert!(route_is_supported(&[
            RoutePos::new(0, 0),
            RoutePos::new(6, 4),
        ]));
        assert!(route_is_supported(&[
            RoutePos::new(0, 0),
            RoutePos::new(6, 0),
            RoutePos::new(6, 4),
        ]));
        assert!(route_is_supported(&[
            RoutePos::new(0, 0),
            RoutePos::new(2, 2),
            RoutePos::new(6, 2),
        ]));
        assert!(!route_is_supported(&[
            RoutePos::new(0, 0),
            RoutePos::new(0, 0),
            RoutePos::new(6, 2),
        ]));
    }

    #[test]
    fn short_segment_warning_ignores_first_and_last_segments() {
        assert!(!has_short_interior_segment(&[
            RoutePos::new(0, 0),
            RoutePos::new(1, 0),
            RoutePos::new(4, 0),
        ]));
        assert!(!has_short_interior_segment(&[
            RoutePos::new(0, 0),
            RoutePos::new(3, 0),
            RoutePos::new(4, 0),
        ]));
        assert!(has_short_interior_segment(&[
            RoutePos::new(0, 0),
            RoutePos::new(2, 0),
            RoutePos::new(3, 0),
            RoutePos::new(5, 0),
        ]));
    }

    #[test]
    fn component_clearance_uses_wire_keepouts_not_placement_keepouts() {
        let (mut design, first_id, second_id) = bridge_fixture();
        let blocker = ComponentDefinition::from_toml(
            "schema_version=1\nid='blocker'\ndefinition_version='1'\nname='Elevated'\nallowed_rotations=['deg0']\n[body]\nx=-1\ny=-1\nwidth=4\nheight=2\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=1\n[[keepouts]]\nmin={x=0,y=0}\nmax={x=0,y=1}\n[[wire_keepouts]]\nmin={x=0,y=0}\nmax={x=0,y=1}",
        )
        .unwrap();
        let blocker_id = ComponentInstanceId::new();
        design
            .component_definitions
            .insert(blocker.id.clone(), blocker.clone());
        design.components.insert(
            blocker_id,
            ComponentInstance {
                id: blocker_id,
                definition_id: blocker.id.clone(),
                name: blocker.name.clone(),
                position: HolePos::new(1, 0),
                rotation: Rotation::Deg0,
                properties: BTreeMap::new(),
                notes: String::new(),
            },
        );
        let mut wire = Wire {
            id: WireId::new(),
            net_id: NetId::new(),
            start: TerminalRef::Pin {
                component_id: first_id,
                pin_id: "p".into(),
            },
            end: TerminalRef::Pin {
                component_id: second_id,
                pin_id: "p".into(),
            },
            side: Side::Component,
            color: WireColor::Red,
            vertices: vec![RoutePos::new(0, 1), RoutePos::new(4, 1)],
            locked: false,
            notes: String::new(),
        };

        assert!(route_crosses_unrelated_component(&design, &wire));
        design
            .component_definitions
            .get_mut("blocker")
            .unwrap()
            .wire_keepouts
            .clear();
        wire.vertices = vec![RoutePos::new(0, 0), RoutePos::new(4, 0)];
        assert!(!route_crosses_unrelated_component(&design, &wire));
    }

    #[test]
    fn bridge_completes_a_net_without_a_wire() {
        let (mut design, first_id, second_id) = bridge_fixture();
        let net_id = NetId::new();
        design.nets.insert(
            net_id,
            ElectricalNet {
                id: net_id,
                name: "bridged".into(),
                state: ConnectionState::Accepted,
                pins: BTreeSet::from([
                    PinRef {
                        component_id: first_id,
                        pin_id: "p".into(),
                    },
                    PinRef {
                        component_id: second_id,
                        pin_id: "p".into(),
                    },
                ]),
                notes: String::new(),
            },
        );

        assert!(validate_structure(&design).is_ok());
        assert!(
            diagnostics(&design)
                .iter()
                .all(|item| item.code.0 != "circuit.net_incomplete")
        );
    }

    #[test]
    fn bridge_rejects_a_short_between_two_nets() {
        let (mut design, first_id, second_id) = bridge_fixture();
        for (name, component_id) in [("first", first_id), ("second", second_id)] {
            let net_id = NetId::new();
            design.nets.insert(
                net_id,
                ElectricalNet {
                    id: net_id,
                    name: name.into(),
                    state: ConnectionState::Accepted,
                    pins: BTreeSet::from([PinRef {
                        component_id,
                        pin_id: "p".into(),
                    }]),
                    notes: String::new(),
                },
            );
        }

        assert_eq!(
            validate_structure(&design),
            Err("bridge connects pins assigned to conflicting nets".into())
        );
    }
}
