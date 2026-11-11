use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, NetId, OperationId, PinRef, ProjectId, Rail, RailId, Rotation,
    RoutePos, Side, TerminalRef, Wire, WireColor, WireId,
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
