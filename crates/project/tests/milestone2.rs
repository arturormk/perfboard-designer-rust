use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, NetId, OperationId, PinRef, ProjectId, Rotation, RoutePos, Side,
    TerminalRef, Wire, WireColor, WireId,
};
use perfboard_project::{
    Board, DocumentRevision, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument,
    apply_operation,
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

fn fixture() -> ProjectDocument {
    let definition = ComponentDefinition::from_toml(
        "schema_version=1\nid='test.pin'\ndefinition_version='1'\nname='Pin'\nallowed_rotations=['deg0']\n[body]\nx=-0.5\ny=-0.5\nwidth=1\nheight=1\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
    ).unwrap();
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "routing",
        Board::new("board", BoardSize::new(9, 9).unwrap(), 2.0),
    );
    for (name, x, y) in [("a", 1, 4), ("b", 7, 4), ("c", 4, 1), ("d", 4, 7)] {
        apply(
            &mut document,
            OperationPayload::AddComponent {
                definition: definition.clone(),
                instance: ComponentInstance {
                    id: ComponentInstanceId::new(),
                    definition_id: definition.id.clone(),
                    name: name.into(),
                    position: HolePos::new(x, y),
                    rotation: Rotation::Deg0,
                    properties: BTreeMap::new(),
                    notes: String::new(),
                    shunted: false,
                },
            },
        )
        .unwrap();
    }
    document
}

#[test]
fn proposed_connectivity_requires_explicit_acceptance() {
    let mut document = fixture();
    let net_id = NetId::new();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "proposal".into(),
                state: ConnectionState::Proposed,
                pins: BTreeSet::new(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    assert_eq!(
        document.design.nets[&net_id].state,
        ConnectionState::Proposed
    );
    apply(
        &mut document,
        OperationPayload::AcceptNetProposal { net_id },
    )
    .unwrap();
    assert_eq!(
        document.design.nets[&net_id].state,
        ConnectionState::Accepted
    );
    assert!(
        apply(
            &mut document,
            OperationPayload::AcceptNetProposal { net_id }
        )
        .is_err()
    );
}

#[test]
fn crossing_wires_do_not_join_disconnected_terminal_groups() {
    let mut document = fixture();
    let ids: Vec<_> = document.design.components.keys().copied().collect();
    let pins: BTreeSet<_> = ids
        .iter()
        .map(|id| PinRef {
            component_id: *id,
            pin_id: "p".into(),
        })
        .collect();
    let net_id = NetId::new();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "crossing".into(),
                state: ConnectionState::Accepted,
                pins: pins.clone(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    let by_name: BTreeMap<_, _> = document
        .design
        .components
        .values()
        .map(|value| (value.name.clone(), value.id))
        .collect();
    for (start_name, end_name, vertices) in [
        ("a", "b", vec![RoutePos::new(2, 8), RoutePos::new(14, 8)]),
        ("c", "d", vec![RoutePos::new(8, 2), RoutePos::new(8, 14)]),
    ] {
        let terminal = |name: &str| TerminalRef::Pin {
            component_id: by_name[name],
            pin_id: "p".into(),
        };
        apply(
            &mut document,
            OperationPayload::AddWire {
                wire: Wire {
                    id: WireId::new(),
                    net_id,
                    start: terminal(start_name),
                    end: terminal(end_name),
                    side: Side::Solder,
                    color: WireColor::Green,
                    vertices,
                    locked: false,
                    notes: String::new(),
                },
            },
        )
        .unwrap();
    }
    assert!(
        document
            .validation
            .iter()
            .any(|item| item.code.0 == "circuit.net_incomplete")
    );
}

#[test]
fn conflicting_pin_assignment_is_atomic() {
    let mut document = fixture();
    let pin = PinRef {
        component_id: *document.design.components.keys().next().unwrap(),
        pin_id: "p".into(),
    };
    for name in ["one", "two"] {
        let id = NetId::new();
        let pins = if name == "one" {
            BTreeSet::from([pin.clone()])
        } else {
            BTreeSet::new()
        };
        apply(
            &mut document,
            OperationPayload::DefineNet {
                net: ElectricalNet {
                    id,
                    name: name.into(),
                    state: ConnectionState::Accepted,
                    pins,
                    notes: String::new(),
                },
            },
        )
        .unwrap();
        if name == "two" {
            let revision = document.revision;
            assert!(
                apply(
                    &mut document,
                    OperationPayload::AssignPin {
                        net_id: id,
                        pin: pin.clone()
                    }
                )
                .is_err()
            );
            assert_eq!(document.revision, revision);
        }
    }
}

#[test]
// Segments may run at any angle (BLUEPRINT 8.2), but never have zero length.
fn zero_length_wire_segment_is_rejected_atomically() {
    let mut document = fixture();
    let by_name: BTreeMap<_, _> = document
        .design
        .components
        .values()
        .map(|value| (value.name.clone(), value.id))
        .collect();
    let start = PinRef {
        component_id: by_name["a"],
        pin_id: "p".into(),
    };
    let end = PinRef {
        component_id: by_name["b"],
        pin_id: "p".into(),
    };
    let net_id = NetId::new();
    apply(
        &mut document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id: net_id,
                name: "bad route".into(),
                state: ConnectionState::Accepted,
                pins: BTreeSet::from([start.clone(), end.clone()]),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    let revision = document.revision;
    assert!(
        apply(
            &mut document,
            OperationPayload::AddWire {
                wire: Wire {
                    id: WireId::new(),
                    net_id,
                    start: start.into(),
                    end: end.into(),
                    side: Side::Component,
                    color: WireColor::White,
                    vertices: vec![
                        RoutePos::new(2, 8),
                        RoutePos::new(2, 8),
                        RoutePos::new(14, 8),
                    ],
                    locked: false,
                    notes: String::new(),
                }
            }
        )
        .is_err()
    );
    assert_eq!(document.revision, revision);
    assert!(document.design.wires.is_empty());
}

#[test]
fn old_documents_default_new_collections() {
    let json = include_str!("../../../tests/fixtures/project-v1/empty-1x1.perfboard.json");
    let document = ProjectDocument::from_json(json).unwrap();
    assert_eq!(document.revision, DocumentRevision(0));
    assert!(document.design.nets.is_empty() && document.design.wires.is_empty());
}

#[test]
fn permanent_example_contains_four_complete_manual_routes() {
    let json = include_str!("../../../examples/nano-rs485.perfboard.json");
    let document = ProjectDocument::from_json(json).unwrap();
    assert_eq!(document.design.nets.len(), 4);
    assert_eq!(document.design.wires.len(), 4);
    assert!(
        !document
            .validation
            .iter()
            .any(|item| item.code.0 == "circuit.net_incomplete")
    );
}

#[test]
fn nets_can_be_renamed_to_unique_non_empty_names() {
    let mut document = fixture();
    let define = |document: &mut ProjectDocument, name: &str| {
        let id = NetId::new();
        apply(
            document,
            OperationPayload::DefineNet {
                net: ElectricalNet {
                    id,
                    name: name.into(),
                    state: ConnectionState::Accepted,
                    pins: BTreeSet::new(),
                    notes: String::new(),
                },
            },
        )
        .unwrap();
        id
    };
    let vcc = define(&mut document, "VCC");
    define(&mut document, "GND");
    let rename = |document: &mut ProjectDocument, name: &str| {
        apply(
            document,
            OperationPayload::RenameNet {
                net_id: vcc,
                name: name.into(),
            },
        )
    };
    rename(&mut document, "  5V ").unwrap();
    assert_eq!(document.design.nets[&vcc].name, "5V");
    // Changing only the case of its own name is fine.
    rename(&mut document, "5v").unwrap();
    assert!(rename(&mut document, "   ").is_err());
    assert!(rename(&mut document, "gnd").is_err());
    assert_eq!(document.design.nets[&vcc].name, "5v");
    assert!(
        apply(
            &mut document,
            OperationPayload::RenameNet {
                net_id: NetId::new(),
                name: "X".into(),
            },
        )
        .is_err()
    );
}
