use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ConnectionState, Diagnostic, DiagnosticCode, DiagnosticSeverity, NetId,
    ObjectReference, PinRef, RoutePos, TerminalRef,
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
        for vertex in &wire.vertices {
            if !route_inside(design.board.size, *vertex) {
                return Err("wire vertex lies outside the board".into());
            }
        }
        for pair in wire.vertices.windows(2) {
            let dx = (pair[1].x2 - pair[0].x2).abs();
            let dy = (pair[1].y2 - pair[0].y2).abs();
            if (dx == 0 && dy == 0) || (dx != 0 && dy != 0 && dx != dy) {
                return Err("wire segments must follow one of eight directions".into());
            }
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
                "Component-side wire passes through another component body or keepout".into(),
                "wire",
                wire.id.to_string(),
            ));
        }
        for pair in wire.vertices.windows(2) {
            let dx = (pair[1].x2 - pair[0].x2).abs();
            let dy = (pair[1].y2 - pair[0].y2).abs();
            if dx.max(dy) < 2 {
                result.push(diag(
                    "route.short_segment",
                    DiagnosticSeverity::Warning,
                    "Wire contains a segment shorter than one grid interval".into(),
                    "wire",
                    wire.id.to_string(),
                ));
                break;
            }
        }
    }
    result.sort_by(|a, b| a.code.cmp(&b.code).then(a.message.cmp(&b.message)));
    result
}

fn route_crosses_unrelated_component(design: &Design, wire: &perfboard_core::Wire) -> bool {
    let endpoint_components: BTreeSet<_> = [&wire.start, &wire.end]
        .into_iter()
        .filter_map(|terminal| match terminal {
            TerminalRef::Pin { component_id, .. } => Some(*component_id),
            TerminalRef::Junction { .. } => None,
        })
        .collect();
    let blocked: BTreeSet<_> = design
        .components
        .values()
        .filter(|instance| !endpoint_components.contains(&instance.id))
        .flat_map(|instance| {
            let definition = &design.component_definitions[&instance.definition_id];
            definition
                .keepouts
                .iter()
                .flat_map(|area| {
                    (area.min.x..=area.max.x)
                        .flat_map(move |x| (area.min.y..=area.max.y).map(move |y| (x, y)))
                })
                .map(|(x, y)| {
                    let (x, y) = instance.rotation.rotate_local((x, y));
                    (
                        (i64::from(instance.position.x) + i64::from(x)) * 2,
                        (i64::from(instance.position.y) + i64::from(y)) * 2,
                    )
                })
        })
        .collect();
    wire.vertices.windows(2).any(|pair| {
        let dx = (pair[1].x2 - pair[0].x2).signum();
        let dy = (pair[1].y2 - pair[0].y2).signum();
        let steps = (pair[1].x2 - pair[0].x2)
            .abs()
            .max((pair[1].y2 - pair[0].y2).abs());
        (1..steps).any(|step| blocked.contains(&(pair[0].x2 + dx * step, pair[0].y2 + dy * step)))
    })
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
    map
}

fn pin_position(design: &Design, pin: &PinRef) -> Option<RoutePos> {
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

fn terminal_position(
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
