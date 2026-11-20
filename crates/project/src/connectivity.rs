use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentInstanceId, ComponentKind, ConnectionState, Diagnostic, DiagnosticCode,
    DiagnosticSeverity, NetId, ObjectReference, PinRef, RoutePos, TerminalRef,
};

use crate::Design;

/// Integrity checks: what the data model cannot represent. These refuse an
/// edit or a file outright, because the board could not be drawn or reasoned
/// about (dangling references, degenerate routes, a pin in two nets).
///
/// Design rules that a correct board must also meet (no shorts, routes on the
/// board, wires on their own net) are reported by [`rule_diagnostics`]
/// instead, so a project that breaks them still opens and can be repaired,
/// and a batch edit can pass through them.
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
        terminal_exists(design, &wire.start)?;
        terminal_exists(design, &wire.end)?;
        for pair in wire.vertices.windows(2) {
            if pair[0] == pair[1] {
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
        if rail.vertices.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("rail segments must have non-zero length".into());
        }
    }
    Ok(())
}

/// Where a wire end's terminal is, whatever net it belongs to.
fn terminal_point(design: &Design, terminal: &TerminalRef) -> Option<RoutePos> {
    match terminal {
        TerminalRef::Pin {
            component_id,
            pin_id,
        } => pin_position(
            design,
            &PinRef {
                component_id: *component_id,
                pin_id: pin_id.clone(),
            },
        ),
        TerminalRef::Junction { junction_id } => design
            .junctions
            .get(junction_id)
            .map(|junction| junction.position.as_route_pos()),
        TerminalRef::RailNode { rail_id, position } => design
            .rails
            .get(rail_id)
            .filter(|rail| rail.vertices.contains(position))
            .map(|_| *position),
    }
}

/// Whether the object a wire end refers to exists.
fn terminal_exists(design: &Design, terminal: &TerminalRef) -> Result<(), String> {
    match terminal {
        TerminalRef::Pin {
            component_id,
            pin_id,
        } => pin_position(
            design,
            &PinRef {
                component_id: *component_id,
                pin_id: pin_id.clone(),
            },
        )
        .map(|_| ())
        .ok_or_else(|| "wire refers to a missing pin".into()),
        TerminalRef::Junction { junction_id } => design
            .junctions
            .contains_key(junction_id)
            .then_some(())
            .ok_or_else(|| "wire refers to a missing junction".into()),
        TerminalRef::RailNode { rail_id, position } => {
            let rail = design
                .rails
                .get(rail_id)
                .ok_or("wire refers to a missing rail")?;
            if rail.vertices.contains(position) {
                Ok(())
            } else {
                Err("wire rail connection is not an explicit rail node".into())
            }
        }
    }
}

/// Design-rule violations, as error diagnostics. A live edit may not add one;
/// a batch edit, or a file saved mid-rework, may contain them.
pub(crate) fn rule_diagnostics(design: &Design) -> Vec<Diagnostic> {
    let mut result = bridge_shorts(design);
    junction_rules(design, &mut result);
    wire_rules(design, &mut result);
    rail_rules(design, &mut result);
    result
}

fn junction_rules(design: &Design, result: &mut Vec<Diagnostic>) {
    let board = design.board.size;
    for junction in design.junctions.values() {
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
            result.push(rule(
                "route.junction_needs_through_hole",
                "Wires on both faces meet at a junction that is not a through-hole".into(),
                "junction",
                junction.id.to_string(),
            ));
        }
        if !board.contains_hole(junction.position) {
            result.push(rule(
                "route.off_board",
                "Junction lies outside the board".into(),
                "junction",
                junction.id.to_string(),
            ));
        }
    }
}

fn wire_rules(design: &Design, result: &mut Vec<Diagnostic>) {
    let board = design.board.size;
    for wire in design.wires.values() {
        let id = wire.id.to_string();
        for terminal in [&wire.start, &wire.end] {
            let mismatch = match terminal {
                TerminalRef::Pin {
                    component_id,
                    pin_id,
                } => design.nets.get(&wire.net_id).is_some_and(|net| {
                    !net.pins.contains(&PinRef {
                        component_id: *component_id,
                        pin_id: pin_id.clone(),
                    })
                }),
                TerminalRef::Junction { junction_id } => design
                    .junctions
                    .get(junction_id)
                    .is_some_and(|junction| junction.net_id != wire.net_id),
                TerminalRef::RailNode { rail_id, .. } => design
                    .rails
                    .get(rail_id)
                    .is_some_and(|rail| rail.net_id != wire.net_id),
            };
            if mismatch {
                result.push(rule(
                    "route.terminal_net_mismatch",
                    "Wire ends on a pin, junction or rail of another net".into(),
                    "wire",
                    id.clone(),
                ));
            }
            // A rail node on a whole hole is soldered through that hole, so
            // a wire on the other face can attach to it like a component pin.
            // A node between holes is reachable only from the rail's own face.
            if let TerminalRef::RailNode { rail_id, position } = terminal
                && design
                    .rails
                    .get(rail_id)
                    .is_some_and(|rail| rail.side != wire.side)
                && !position.is_hole()
            {
                result.push(rule(
                    "route.rail_node_other_side",
                    "A wire on the other face can only reach a rail node on a whole hole".into(),
                    "wire",
                    id.clone(),
                ));
            }
        }
        if wire
            .vertices
            .iter()
            .any(|vertex| !board.reaches_wire_vertex(*vertex))
        {
            result.push(rule(
                "route.off_board",
                "Wire runs beyond the board margin".into(),
                "wire",
                id.clone(),
            ));
        }
        if route_crosses_unrelated_pin(design, wire) {
            result.push(rule(
                "route.crosses_pin",
                "Wire passes through a pin it does not connect to".into(),
                "wire",
                id,
            ));
        }
    }
}

fn rail_rules(design: &Design, result: &mut Vec<Diagnostic>) {
    let board = design.board.size;
    for rail in design.rails.values() {
        if rail
            .vertices
            .iter()
            .any(|vertex| !route_inside(board, *vertex))
        {
            result.push(rule(
                "route.off_board",
                "Rail runs outside the board".into(),
                "rail",
                rail.id.to_string(),
            ));
        }
        if rail_crosses_pin(design, rail) {
            result.push(rule(
                "route.crosses_pin",
                "Rail passes through a component pin".into(),
                "rail",
                rail.id.to_string(),
            ));
        }
    }
}

fn rule(code: &str, message: String, kind: &str, id: String) -> Diagnostic {
    diag(code, DiagnosticSeverity::Error, message, kind, id)
}

/// A run of touching bridges that joins pins of more than one net.
fn bridge_shorts(design: &Design) -> Vec<Diagnostic> {
    let groups = bridge_groups(design);
    let mut nets_by_group = BTreeMap::<RoutePos, BTreeSet<NetId>>::new();
    for (net_id, net) in &design.nets {
        for pin in &net.pins {
            if let Some(position) = pin_position(design, pin)
                && let Some(group) = groups.get(&position)
            {
                nets_by_group.entry(*group).or_default().insert(*net_id);
            }
        }
    }
    let mut result = Vec::new();
    for (group, nets) in nets_by_group {
        if nets.len() < 2 {
            continue;
        }
        let names = nets
            .iter()
            .map(|id| format!("`{}`", design.nets[id].name))
            .collect::<Vec<_>>()
            .join(", ");
        let mut objects: Vec<ObjectReference> = bridges_in_group(design, &groups, group)
            .into_iter()
            .map(|id| ObjectReference {
                kind: "component_instance".into(),
                id: id.to_string(),
            })
            .collect();
        objects.extend(nets.iter().map(|id| ObjectReference {
            kind: "net".into(),
            id: id.to_string(),
        }));
        result.push(Diagnostic {
            code: DiagnosticCode("circuit.bridge_short".into()),
            severity: DiagnosticSeverity::Error,
            message: format!("Bridges short nets {names} together"),
            objects,
        });
    }
    result
}

fn bridge_ends(
    instance: &perfboard_core::ComponentInstance,
    definition: &perfboard_core::ComponentDefinition,
) -> Vec<RoutePos> {
    definition
        .pins
        .iter()
        .map(|pin| {
            let (x, y) = instance.rotation.rotate_local((pin.x, pin.y));
            RoutePos::new(
                (i64::from(instance.position.x) + i64::from(x)) * 2,
                (i64::from(instance.position.y) + i64::from(y)) * 2,
            )
        })
        .collect()
}

fn bridges_in_group(
    design: &Design,
    groups: &BTreeMap<RoutePos, RoutePos>,
    group: RoutePos,
) -> Vec<ComponentInstanceId> {
    design
        .components
        .values()
        .filter(|instance| {
            design
                .component_definitions
                .get(&instance.definition_id)
                .is_some_and(|definition| {
                    definition.kind == ComponentKind::Bridge
                        && bridge_ends(instance, definition)
                            .first()
                            .is_some_and(|end| groups.get(end) == Some(&group))
                })
        })
        .map(|instance| instance.id)
        .collect()
}

/// Every jumper whose shunt is fitted, with its two pins.
pub(crate) fn shunted_jumpers(
    design: &Design,
) -> impl Iterator<Item = (&perfboard_core::ComponentInstance, [PinRef; 2])> {
    design.components.values().filter_map(|instance| {
        if !instance.shunted {
            return None;
        }
        let definition = design.component_definitions.get(&instance.definition_id)?;
        if definition.kind != ComponentKind::Jumper {
            return None;
        }
        let [first, second] = definition.pins.as_slice() else {
            return None;
        };
        let pin = |id: &str| PinRef {
            component_id: instance.id,
            pin_id: id.to_owned(),
        };
        Some((instance, [pin(&first.id), pin(&second.id)]))
    })
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
    result.extend(shunt_join_advisories(design));
    result.extend(rule_diagnostics(design));
    for wire in design.wires.values() {
        // Position only: a terminal on the wrong net is its own rule error.
        let endpoints_match = terminal_point(design, &wire.start)
            .zip(terminal_point(design, &wire.end))
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

/// Electrical connectivity of one net's assigned pins.
///
/// `islands` partitions the net's pins into groups that are already joined by
/// wires, rails, bridges, or coincident pin positions. A fully routed net has
/// exactly one island. The UI uses this to draw unrouted connections without
/// re-implementing the connectivity rules.
///
/// A shunted jumper whose pins sit on two different nets joins them while the
/// shunt is fitted. Each net stays its own native net, routed and checked on
/// its own copper; `joined_nets` is the computed view: every other net this
/// one currently reaches through shunts (transitively), and `shunts` the
/// jumpers doing the joining.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct NetConnectivity {
    pub net_id: NetId,
    pub islands: Vec<Vec<PinRef>>,
    pub joined_nets: Vec<NetId>,
    pub shunts: Vec<ComponentInstanceId>,
}

/// Computes pin islands, and nets joined through shunts, for every accepted
/// net.
#[must_use]
pub fn net_connectivity(design: &Design) -> Vec<NetConnectivity> {
    let joins = shunt_joins(design);
    design
        .nets
        .values()
        .filter(|net| net.state == ConnectionState::Accepted)
        .map(|net| {
            let adjacency = adjacency(design, net.id);
            let mut assigned = BTreeSet::new();
            let mut islands = Vec::new();
            for pin in &net.pins {
                let start = TerminalRef::from(pin.clone());
                if assigned.contains(&start) {
                    continue;
                }
                let mut reached = BTreeSet::new();
                let mut pending = vec![start];
                while let Some(current) = pending.pop() {
                    if !reached.insert(current.clone()) {
                        continue;
                    }
                    if let Some(next) = adjacency.get(&current) {
                        pending.extend(next.iter().cloned());
                    }
                }
                let island = net
                    .pins
                    .iter()
                    .filter(|candidate| reached.contains(&TerminalRef::from((*candidate).clone())))
                    .cloned()
                    .collect::<Vec<_>>();
                assigned.extend(reached);
                islands.push(island);
            }
            let mut joined_nets = BTreeSet::new();
            let mut pending = vec![net.id];
            while let Some(current) = pending.pop() {
                for (first, second, _) in &joins {
                    let other = if *first == current {
                        *second
                    } else if *second == current {
                        *first
                    } else {
                        continue;
                    };
                    if other != net.id && joined_nets.insert(other) {
                        pending.push(other);
                    }
                }
            }
            let shunts = joins
                .iter()
                .filter(|(first, second, _)| {
                    *first == net.id
                        || *second == net.id
                        || (joined_nets.contains(first) && joined_nets.contains(second))
                })
                .map(|(_, _, via)| *via)
                .collect::<BTreeSet<_>>();
            NetConnectivity {
                net_id: net.id,
                islands,
                joined_nets: joined_nets.into_iter().collect(),
                shunts: shunts.into_iter().collect(),
            }
        })
        .collect()
}

/// An advisory for each fitted shunt that joins two different nets. Joining
/// nets is what a selector jumper is for, so this is never an error; it keeps
/// the join visible in the validation report.
fn shunt_join_advisories(design: &Design) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for (instance, pins) in shunted_jumpers(design) {
        let [Some(first), Some(second)] = pins.map(|pin| net_of_pin(design, &pin)) else {
            continue;
        };
        if first.id == second.id {
            continue;
        }
        result.push(Diagnostic {
            code: DiagnosticCode("circuit.shunt_joins_nets".into()),
            severity: DiagnosticSeverity::Advisory,
            message: format!(
                "Shunt on {} joins nets `{}` and `{}`",
                instance.name, first.name, second.name
            ),
            objects: vec![
                ObjectReference {
                    kind: "component".into(),
                    id: instance.id.to_string(),
                },
                ObjectReference {
                    kind: "net".into(),
                    id: first.id.to_string(),
                },
                ObjectReference {
                    kind: "net".into(),
                    id: second.id.to_string(),
                },
            ],
        });
    }
    result
}

/// Shunted jumpers that join two different accepted nets: `(net, net, jumper)`.
fn shunt_joins(design: &Design) -> Vec<(NetId, NetId, ComponentInstanceId)> {
    shunted_jumpers(design)
        .filter_map(|(instance, pins)| {
            let [Some(first), Some(second)] = pins.map(|pin| net_of_pin(design, &pin)) else {
                return None;
            };
            (first.id != second.id
                && first.state == ConnectionState::Accepted
                && second.state == ConnectionState::Accepted)
                .then_some((first.id, second.id, instance.id))
        })
        .collect()
}

fn net_of_pin<'a>(design: &'a Design, pin: &PinRef) -> Option<&'a perfboard_core::ElectricalNet> {
    design.nets.values().find(|net| net.pins.contains(pin))
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
    // Bridges are bare copper: a run of touching bridges joins every pin at
    // any of its ends, whether or not the bridge ends themselves are listed
    // in the net.
    let groups = bridge_groups(design);
    let group_of = |position: &RoutePos| groups.get(position).copied().unwrap_or(*position);
    for (index, (first, first_position)) in terminals.iter().enumerate() {
        for (second, second_position) in terminals.iter().skip(index + 1) {
            if first_position != second_position
                && group_of(first_position) == group_of(second_position)
            {
                add_connection(&mut map, first, second);
            }
        }
    }
    if let Some(net) = design.nets.get(&net_id) {
        for (_, [first, second]) in shunted_jumpers(design) {
            if net.pins.contains(&first) && net.pins.contains(&second) {
                add_connection(&mut map, &first.into(), &second.into());
            }
        }
    }
    map
}

/// Maps each bridge end to a representative position shared by every end
/// reachable from it through touching bridges.
fn bridge_groups(design: &Design) -> BTreeMap<RoutePos, RoutePos> {
    fn root(parent: &mut BTreeMap<RoutePos, RoutePos>, position: RoutePos) -> RoutePos {
        let mut current = position;
        while let Some(&next) = parent.get(&current) {
            if next == current {
                break;
            }
            current = next;
        }
        parent.insert(position, current);
        current
    }
    let mut parent = BTreeMap::new();
    for instance in design.components.values() {
        let Some(definition) = design.component_definitions.get(&instance.definition_id) else {
            continue;
        };
        if definition.kind != ComponentKind::Bridge {
            continue;
        }
        let mut ends = bridge_ends(instance, definition).into_iter();
        let Some(first) = ends.next() else {
            continue;
        };
        parent.entry(first).or_insert(first);
        for end in ends {
            parent.entry(end).or_insert(end);
            let (a, b) = (root(&mut parent, first), root(&mut parent, end));
            if a != b {
                parent.insert(a.max(b), a.min(b));
            }
        }
    }
    let positions: Vec<_> = parent.keys().copied().collect();
    positions
        .into_iter()
        .map(|position| (position, root(&mut parent, position)))
        .collect()
}

fn add_connection(
    map: &mut BTreeMap<TerminalRef, BTreeSet<TerminalRef>>,
    first: &TerminalRef,
    second: &TerminalRef,
) {
    map.entry(first.clone()).or_default().insert(second.clone());
    map.entry(second.clone()).or_default().insert(first.clone());
}

/// Lets the bridges joined to `pin` follow its new net `net_id`.
///
/// A bridge end is added to a net when a wire is drawn to it, and keeps that
/// membership after the wire is removed. Such a stale membership would block
/// giving the bridge's neighbour a new net, so every bridge pin reachable from
/// `pin` (through chained bridges) that belongs to another net and has no wire
/// attached is released from it. A bridge pin that still carries a wire keeps
/// its net, and the bridge conflict check then rejects the change: that is a
/// real short between two nets.
pub(crate) fn release_stale_bridge_pins(design: &mut Design, net_id: NetId, pin: &PinRef) {
    let Some(start) = pin_position(design, pin) else {
        return;
    };
    let bridges: Vec<(perfboard_core::ComponentInstanceId, Vec<(String, RoutePos)>)> = design
        .components
        .values()
        .filter_map(|instance| {
            let definition = design.component_definitions.get(&instance.definition_id)?;
            (definition.kind == ComponentKind::Bridge).then(|| {
                let ends = definition
                    .pins
                    .iter()
                    .map(|bridge_pin| {
                        let (x, y) = instance.rotation.rotate_local((bridge_pin.x, bridge_pin.y));
                        (
                            bridge_pin.id.clone(),
                            RoutePos::new(
                                (i64::from(instance.position.x) + i64::from(x)) * 2,
                                (i64::from(instance.position.y) + i64::from(y)) * 2,
                            ),
                        )
                    })
                    .collect();
                (instance.id, ends)
            })
        })
        .collect();

    let mut reached = BTreeSet::from([start]);
    let mut joined = BTreeSet::new();
    loop {
        let before = reached.len();
        for (index, (_, ends)) in bridges.iter().enumerate() {
            if ends.iter().any(|(_, position)| reached.contains(position)) {
                joined.insert(index);
                reached.extend(ends.iter().map(|(_, position)| *position));
            }
        }
        if reached.len() == before {
            break;
        }
    }

    let wired: BTreeSet<PinRef> = design
        .wires
        .values()
        .flat_map(|wire| [&wire.start, &wire.end])
        .filter_map(|terminal| match terminal {
            TerminalRef::Pin {
                component_id,
                pin_id,
            } => Some(PinRef {
                component_id: *component_id,
                pin_id: pin_id.clone(),
            }),
            _ => None,
        })
        .collect();
    for index in joined {
        let (component_id, ends) = &bridges[index];
        for (pin_id, _) in ends {
            let bridge_pin = PinRef {
                component_id: *component_id,
                pin_id: pin_id.clone(),
            };
            if bridge_pin == *pin || wired.contains(&bridge_pin) {
                continue;
            }
            for (other_id, net) in &mut design.nets {
                if *other_id != net_id {
                    net.pins.remove(&bridge_pin);
                }
            }
        }
    }
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

/// Splits a segment into its smallest repeated grid step: returns
/// `(step_x, step_y, count)` so that the points `start + step * i` for
/// `i in 0..=count` are every grid point on the segment. A zero-length segment
/// yields `(0, 0, 0)`.
pub(crate) fn segment_step(pair: &[RoutePos]) -> (i64, i64, i64) {
    let delta_x = pair[1].x2 - pair[0].x2;
    let delta_y = pair[1].y2 - pair[0].y2;
    // The gcd divides both deltas, so it is at most |delta| and fits in i64.
    let steps = gcd(delta_x.unsigned_abs(), delta_y.unsigned_abs()).cast_signed();
    if steps == 0 {
        (0, 0, 0)
    } else {
        (delta_x / steps, delta_y / steps, steps)
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
                    value: String::new(),
                    properties: BTreeMap::new(),
                    notes: String::new(),
                    shunted: false,
                    component_side: false,
                    pin_labels: BTreeMap::new(),
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
                value: String::new(),
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
                component_side: false,
                pin_labels: BTreeMap::new(),
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
    fn chained_bridges_complete_a_net_without_their_ends_in_it() {
        // Pins at (0,0) and (2,0), joined by bridges (0,0)-(1,0) and
        // (1,0)-(2,0). No bridge end is listed in the net.
        let (mut design, first_id, second_id) = bridge_fixture();
        design.components.get_mut(&second_id).unwrap().position = HolePos::new(2, 0);
        let second_bridge = ComponentInstanceId::new();
        design.components.insert(
            second_bridge,
            ComponentInstance {
                id: second_bridge,
                definition_id: "bridge".into(),
                name: "bridge 2".into(),
                position: HolePos::new(1, 0),
                rotation: Rotation::Deg0,
                value: String::new(),
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
                component_side: false,
                pin_labels: BTreeMap::new(),
            },
        );
        let net_id = NetId::new();
        design.nets.insert(
            net_id,
            ElectricalNet {
                id: net_id,
                name: "chained".into(),
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

        assert!(
            diagnostics(&design)
                .iter()
                .all(|item| item.code.0 != "circuit.net_incomplete")
        );
        assert_eq!(net_connectivity(&design)[0].islands.len(), 1);

        design.components.remove(&second_bridge);
        assert!(
            diagnostics(&design)
                .iter()
                .any(|item| item.code.0 == "circuit.net_incomplete")
        );
    }

    #[test]
    fn net_connectivity_groups_pins_into_islands() {
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

        let bridged = net_connectivity(&design);
        assert_eq!(bridged.len(), 1);
        assert_eq!(bridged[0].islands.len(), 1);
        assert_eq!(bridged[0].islands[0].len(), 2);

        let bridge_ids = design
            .components
            .values()
            .filter(|instance| {
                design.component_definitions[&instance.definition_id].kind == ComponentKind::Bridge
            })
            .map(|instance| instance.id)
            .collect::<Vec<_>>();
        for id in bridge_ids {
            design.components.remove(&id);
        }
        let split = net_connectivity(&design);
        assert_eq!(split[0].islands.len(), 2);
    }

    #[test]
    fn bridge_short_between_two_nets_is_a_rule_error() {
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

        // Still a loadable design, so it can be opened and repaired.
        assert_eq!(validate_structure(&design), Ok(()));
        let shorts: Vec<_> = rule_diagnostics(&design)
            .into_iter()
            .filter(|item| item.code.0 == "circuit.bridge_short")
            .collect();
        assert_eq!(shorts.len(), 1);
        assert_eq!(shorts[0].severity, DiagnosticSeverity::Error);
    }
}
