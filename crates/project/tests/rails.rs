use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, Junction, JunctionId, NetId, OperationId, PinRef, ProjectId, Rail,
    RailId, Rotation, RoutePos, Side, TerminalRef, Wire, WireColor, WireId,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument, apply_operation,
};

fn apply(document: &mut ProjectDocument, payload: OperationPayload) -> Result<(), String> {
    apply_operation(
        document,
        OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: document.revision,
            origin: OperationOrigin::User,
            rationale: None,
            allow_invalid_placement: false,
            payload,
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[test]
#[allow(clippy::too_many_lines)] // One end-to-end rail scenario, kept in order.
fn wires_connect_through_explicit_nodes_on_a_net_owned_rail() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "rails",
        Board::new("board", BoardSize::new(8, 8).unwrap(), 2.54),
    );
    let net_id = NetId::new();
    let definition = ComponentDefinition::from_toml(
        "schema_version=1\nid='pin'\ndefinition_version='1'\nname='Pin'\nallowed_rotations=['deg0']\n[body]\nx=-0.2\ny=-0.2\nwidth=0.4\nheight=0.4\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
    )
    .unwrap();
    let component_ids = [ComponentInstanceId::new(), ComponentInstanceId::new()];
    for (id, x) in component_ids.into_iter().zip([0, 6]) {
        apply(
            &mut document,
            OperationPayload::AddComponent {
                definition: definition.clone(),
                instance: ComponentInstance {
                    id,
                    definition_id: definition.id.clone(),
                    name: "pin".into(),
                    position: HolePos::new(x, 1),
                    rotation: Rotation::Deg0,
                    properties: BTreeMap::new(),
                    notes: String::new(),
                    shunted: false,
                },
            },
        )
        .unwrap();
    }
    let pins = component_ids
        .into_iter()
        .map(|component_id| PinRef {
            component_id,
            pin_id: "p".into(),
        })
        .collect::<BTreeSet<_>>();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "power".into(),
                state: ConnectionState::Accepted,
                pins: pins.clone(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    let rail_id = RailId::new();
    let nodes = [
        RoutePos::new(2, 3),
        RoutePos::new(6, 3),
        RoutePos::new(10, 3),
    ];
    apply(
        &mut document,
        OperationPayload::AddRail {
            rail: Rail {
                id: rail_id,
                net_id,
                side: Side::Solder,
                color: WireColor::Red,
                vertices: nodes.to_vec(),
                locked: false,
                notes: String::new(),
            },
        },
    )
    .unwrap();

    for (pin, node) in pins.into_iter().zip([nodes[0], nodes[2]]) {
        let pin_position = if pin.component_id == component_ids[0] {
            RoutePos::new(0, 2)
        } else {
            RoutePos::new(12, 2)
        };
        apply(
            &mut document,
            OperationPayload::AddWire {
                wire: Wire {
                    id: WireId::new(),
                    net_id,
                    start: TerminalRef::from(pin),
                    end: TerminalRef::RailNode {
                        rail_id,
                        position: node,
                    },
                    side: Side::Solder,
                    color: WireColor::Green,
                    vertices: vec![pin_position, node],
                    locked: false,
                    notes: String::new(),
                },
            },
        )
        .unwrap();
    }
    document.validate().unwrap();
    assert!(
        !document
            .validation
            .iter()
            .any(|diagnostic| diagnostic.code.0 == "circuit.net_incomplete")
    );

    assert!(
        apply(
            &mut document,
            OperationPayload::ModifyRailPath {
                rail_id,
                vertices: vec![nodes[0], nodes[1]],
            },
        )
        .unwrap_err()
        .contains("referenced")
    );
    assert!(
        apply(&mut document, OperationPayload::RemoveRail { rail_id })
            .unwrap_err()
            .contains("referenced")
    );
}

#[test]
fn rail_connections_require_a_stored_half_grid_vertex_and_matching_net() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "rails",
        Board::new("board", BoardSize::new(4, 4).unwrap(), 2.54),
    );
    let net_id = NetId::new();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "signal".into(),
                state: ConnectionState::Accepted,
                pins: BTreeSet::new(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    let rail_id = RailId::new();
    apply(
        &mut document,
        OperationPayload::AddRail {
            rail: Rail {
                id: rail_id,
                net_id,
                side: Side::Component,
                color: WireColor::Blue,
                vertices: vec![RoutePos::new(0, 0), RoutePos::new(4, 0)],
                locked: false,
                notes: String::new(),
            },
        },
    )
    .unwrap();
    let invalid = TerminalRef::RailNode {
        rail_id,
        position: RoutePos::new(2, 0),
    };
    let valid = TerminalRef::RailNode {
        rail_id,
        position: RoutePos::new(4, 0),
    };
    assert!(
        apply(
            &mut document,
            OperationPayload::AddWire {
                wire: Wire {
                    id: WireId::new(),
                    net_id,
                    start: invalid,
                    end: valid,
                    side: Side::Component,
                    color: WireColor::Green,
                    vertices: vec![RoutePos::new(2, 0), RoutePos::new(4, 0)],
                    locked: false,
                    notes: String::new(),
                },
            },
        )
        .is_err()
    );
}

fn net_with_rail(side: Side, vertices: Vec<RoutePos>) -> (ProjectDocument, NetId, RailId) {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "rails",
        Board::new("board", BoardSize::new(6, 6).unwrap(), 2.54),
    );
    let net_id = NetId::new();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "power".into(),
                state: ConnectionState::Accepted,
                pins: BTreeSet::new(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    let rail_id = RailId::new();
    apply(
        &mut document,
        OperationPayload::AddRail {
            rail: Rail {
                id: rail_id,
                net_id,
                side,
                color: WireColor::Red,
                vertices,
                locked: false,
                notes: String::new(),
            },
        },
    )
    .unwrap();
    (document, net_id, rail_id)
}

#[test]
fn rail_color_can_change_even_when_locked() {
    let (mut document, _, rail_id) =
        net_with_rail(Side::Solder, vec![RoutePos::new(0, 0), RoutePos::new(8, 0)]);
    document.design.rails.get_mut(&rail_id).unwrap().locked = true;
    apply(
        &mut document,
        OperationPayload::SetRailColor {
            rail_id,
            color: WireColor::Blue,
        },
    )
    .unwrap();
    assert_eq!(document.design.rails[&rail_id].color, WireColor::Blue);
    assert!(
        apply(
            &mut document,
            OperationPayload::SetRailColor {
                rail_id: RailId::new(),
                color: WireColor::Blue,
            },
        )
        .is_err()
    );
}

#[test]
fn inserted_nodes_take_wires_from_either_side_on_whole_holes() {
    let (mut document, net_id, rail_id) =
        net_with_rail(Side::Solder, vec![RoutePos::new(0, 0), RoutePos::new(8, 0)]);
    // Add a collinear node on hole (2, 0) and one between holes at x2 = 5.
    apply(
        &mut document,
        OperationPayload::ModifyRailPath {
            rail_id,
            vertices: vec![
                RoutePos::new(0, 0),
                RoutePos::new(4, 0),
                RoutePos::new(5, 0),
                RoutePos::new(8, 0),
            ],
        },
    )
    .unwrap();
    // A component-side wire between two nodes of a solder-side rail.
    let wire_between = |from: RoutePos, to: RoutePos, vertices: Vec<RoutePos>| Wire {
        id: WireId::new(),
        net_id,
        start: TerminalRef::RailNode {
            rail_id,
            position: from,
        },
        end: TerminalRef::RailNode {
            rail_id,
            position: to,
        },
        side: Side::Component,
        color: WireColor::Green,
        vertices,
        locked: false,
        notes: String::new(),
    };
    // Both ends on whole holes: soldered through the board.
    let through = wire_between(
        RoutePos::new(4, 0),
        RoutePos::new(0, 0),
        vec![
            RoutePos::new(4, 0),
            RoutePos::new(4, 4),
            RoutePos::new(0, 4),
            RoutePos::new(0, 0),
        ],
    );
    apply(&mut document, OperationPayload::AddWire { wire: through }).unwrap();
    // One end between holes cannot reach the other face.
    let between = wire_between(
        RoutePos::new(5, 0),
        RoutePos::new(8, 0),
        vec![
            RoutePos::new(5, 0),
            RoutePos::new(5, 6),
            RoutePos::new(8, 6),
            RoutePos::new(8, 0),
        ],
    );
    assert!(
        apply(&mut document, OperationPayload::AddWire { wire: between })
            .unwrap_err()
            .contains("whole hole")
    );
}

#[test]
fn notes_on_wires_rails_nets_and_junctions_can_be_edited_even_when_locked() {
    let (mut document, net_id, rail_id) =
        net_with_rail(Side::Solder, vec![RoutePos::new(0, 0), RoutePos::new(8, 0)]);
    let junction_id = JunctionId::new();
    let wire_id = WireId::new();
    document.design.wires.insert(
        wire_id,
        Wire {
            id: wire_id,
            net_id,
            start: TerminalRef::RailNode {
                rail_id,
                position: RoutePos::new(0, 0),
            },
            end: TerminalRef::RailNode {
                rail_id,
                position: RoutePos::new(8, 0),
            },
            side: Side::Solder,
            color: WireColor::Red,
            vertices: vec![RoutePos::new(0, 0), RoutePos::new(8, 0)],
            locked: true,
            notes: "old".into(),
        },
    );
    document.design.rails.get_mut(&rail_id).unwrap().locked = true;

    for payload in [
        OperationPayload::SetWireNotes {
            wire_id,
            notes: "twisted pair with B".into(),
        },
        OperationPayload::SetRailNotes {
            rail_id,
            notes: "bus bar, 0.8 mm tinned".into(),
        },
        OperationPayload::SetNetNotes {
            net_id,
            notes: "keep short".into(),
        },
        OperationPayload::AddJunction {
            junction: Junction {
                id: junction_id,
                net_id,
                position: HolePos::new(3, 3),
                through_hole: false,
                notes: String::new(),
            },
        },
        OperationPayload::SetJunctionNotes {
            junction_id,
            notes: "star ground point".into(),
        },
    ] {
        apply(&mut document, payload).unwrap();
    }
    assert_eq!(document.design.wires[&wire_id].notes, "twisted pair with B");
    assert_eq!(
        document.design.rails[&rail_id].notes,
        "bus bar, 0.8 mm tinned"
    );
    assert_eq!(document.design.nets[&net_id].notes, "keep short");
    assert_eq!(
        document.design.junctions[&junction_id].notes,
        "star ground point"
    );

    apply(
        &mut document,
        OperationPayload::SetWireNotes {
            wire_id,
            notes: String::new(),
        },
    )
    .unwrap();
    assert_eq!(document.design.wires[&wire_id].notes, "");

    for payload in [
        OperationPayload::SetWireNotes {
            wire_id: WireId::new(),
            notes: "x".into(),
        },
        OperationPayload::SetRailNotes {
            rail_id: RailId::new(),
            notes: "x".into(),
        },
        OperationPayload::SetNetNotes {
            net_id: NetId::new(),
            notes: "x".into(),
        },
        OperationPayload::SetJunctionNotes {
            junction_id: JunctionId::new(),
            notes: "x".into(),
        },
    ] {
        assert!(apply(&mut document, payload).is_err());
    }
}
