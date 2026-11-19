use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, NetId, OperationId, PinRef, ProjectId, Rotation, RoutePos, Side,
    TerminalRef, Wire, WireColor, WireId,
};
use perfboard_project::{
    Board, OperationError, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument,
    ProjectSession, TransactionRequest, apply_operation, to_snapshot_json,
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
            defer_checks: false,
            payload,
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

const PIN: &str = "schema_version=1\nid='pin'\ndefinition_version='1'\nname='Pin'\nallowed_rotations=['deg0']\n[body]\nx=-0.2\ny=-0.2\nwidth=0.4\nheight=0.4\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0";
const BRIDGE: &str = "schema_version=1\nkind='bridge'\nid='bridge'\ndefinition_version='1'\nname='Bridge'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=-0.2\nwidth=2.54\nheight=0.4\n[[pins]]\nid='a'\nnumber='A'\nname='A'\nx=0\ny=0\n[[pins]]\nid='b'\nnumber='B'\nname='B'\nx=1\ny=0";

fn place(document: &mut ProjectDocument, toml: &str, x: i32) -> ComponentInstanceId {
    let definition = ComponentDefinition::from_toml(toml).unwrap();
    let id = ComponentInstanceId::new();
    apply(
        document,
        OperationPayload::AddComponent {
            definition: definition.clone(),
            instance: ComponentInstance {
                id,
                definition_id: definition.id.clone(),
                name: definition.name.clone(),
                position: HolePos::new(x, 0),
                rotation: Rotation::Deg0,
                value: String::new(),
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
            },
        },
    )
    .unwrap();
    id
}

fn net(document: &mut ProjectDocument, name: &str, pins: &[PinRef]) -> NetId {
    let id = NetId::new();
    apply(
        document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id,
                name: name.into(),
                state: ConnectionState::Accepted,
                pins: pins.iter().cloned().collect::<BTreeSet<_>>(),
                notes: String::new(),
            },
        },
    )
    .unwrap();
    id
}

fn pin(component_id: ComponentInstanceId, pin_id: &str) -> PinRef {
    PinRef {
        component_id,
        pin_id: pin_id.into(),
    }
}

/// A pin at hole 0, a bridge from hole 0 to hole 1, and a pin at hole 5 on
/// net X wired to the bridge's far end.
struct Fixture {
    document: ProjectDocument,
    near: PinRef,
    bridge_far: PinRef,
    x: NetId,
    y: NetId,
    wire_id: WireId,
}

fn fixture() -> Fixture {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "bridges",
        Board::new("board", BoardSize::new(8, 4).unwrap(), 2.54),
    );
    let near = pin(place(&mut document, PIN, 0), "p");
    let bridge = place(&mut document, BRIDGE, 0);
    let far = pin(place(&mut document, PIN, 5), "p");
    let bridge_far = pin(bridge, "b");
    let x = net(&mut document, "X", std::slice::from_ref(&far));
    let y = net(&mut document, "Y", &[]);
    apply(
        &mut document,
        OperationPayload::AssignPin {
            net_id: x,
            pin: bridge_far.clone(),
        },
    )
    .unwrap();
    let wire_id = WireId::new();
    apply(
        &mut document,
        OperationPayload::AddWire {
            wire: Wire {
                id: wire_id,
                net_id: x,
                start: TerminalRef::from(bridge_far.clone()),
                end: TerminalRef::from(far),
                side: Side::Solder,
                color: WireColor::Green,
                vertices: vec![RoutePos::new(2, 0), RoutePos::new(10, 0)],
                locked: false,
                notes: String::new(),
            },
        },
    )
    .unwrap();
    Fixture {
        document,
        near,
        bridge_far,
        x,
        y,
        wire_id,
    }
}

#[test]
fn a_bridge_follows_its_neighbour_once_its_wire_is_gone() {
    let Fixture {
        mut document,
        near,
        bridge_far,
        x,
        y,
        wire_id,
    } = fixture();
    apply(&mut document, OperationPayload::RemoveWire { wire_id }).unwrap();
    apply(
        &mut document,
        OperationPayload::AssignPin {
            net_id: y,
            pin: near.clone(),
        },
    )
    .unwrap();
    assert!(document.design.nets[&y].pins.contains(&near));
    assert!(!document.design.nets[&x].pins.contains(&bridge_far));

    // The same applies when a new net is defined straight onto the pin.
    apply(
        &mut document,
        OperationPayload::UnassignPin {
            net_id: y,
            pin: near.clone(),
        },
    )
    .unwrap();
    apply(
        &mut document,
        OperationPayload::AssignPin {
            net_id: x,
            pin: bridge_far,
        },
    )
    .unwrap();
    net(&mut document, "Z", &[near]);
}

#[test]
fn a_bridge_still_wired_to_another_net_is_a_short() {
    let Fixture {
        mut document,
        near,
        y,
        ..
    } = fixture();
    let error = apply(
        &mut document,
        OperationPayload::AssignPin {
            net_id: y,
            pin: near,
        },
    )
    .unwrap_err();
    assert!(error.to_lowercase().contains("short"), "{error}");
}

/// A wire from a pin on net N to the free end of a bridge (or a chain of
/// bridges) whose other end shares a hole with another pin on N, drawn on
/// either face: a bridge end is soldered in its hole, so both faces reach it.
#[test]
fn a_wire_can_end_on_a_bridge_joined_to_its_net_from_either_side() {
    for (chain, side) in [1i32, 2]
        .into_iter()
        .flat_map(|chain| [Side::Solder, Side::Component].map(|side| (chain, side)))
    {
        let mut document = ProjectDocument::new(
            ProjectId::new(),
            "bridges",
            Board::new("board", BoardSize::new(10, 4).unwrap(), 2.54),
        );
        let p = pin(place(&mut document, PIN, 0), "p");
        let last = (0..chain)
            .map(|x| place(&mut document, BRIDGE, x))
            .last()
            .unwrap();
        let q = pin(place(&mut document, PIN, 7), "p");
        let n = net(&mut document, "N", &[p, q.clone()]);
        let far = pin(last, "b");
        apply(
            &mut document,
            OperationPayload::AssignPin {
                net_id: n,
                pin: far.clone(),
            },
        )
        .unwrap();
        apply(
            &mut document,
            OperationPayload::AddWire {
                wire: Wire {
                    id: WireId::new(),
                    net_id: n,
                    start: TerminalRef::from(q),
                    end: TerminalRef::from(far),
                    side,
                    color: WireColor::Green,
                    vertices: vec![RoutePos::new(14, 0), RoutePos::new(i64::from(chain) * 2, 0)],
                    locked: false,
                    notes: String::new(),
                },
            },
        )
        .unwrap_or_else(|error| panic!("chain {chain}, {side:?}: {error}"));
    }
}

/// A bridge whose ends joined a net, but carry no wire, can be removed; its
/// ends leave the net with it. One that still has a wire ending on it cannot.
#[test]
fn a_bridge_without_wires_can_be_removed_even_if_its_ends_joined_a_net() {
    let Fixture {
        mut document,
        bridge_far,
        x,
        wire_id,
        ..
    } = fixture();
    let bridge = bridge_far.component_id;
    let error = apply(
        &mut document,
        OperationPayload::RemoveComponent {
            instance_id: bridge,
        },
    )
    .unwrap_err();
    assert!(error.contains("wire"), "{error}");

    apply(&mut document, OperationPayload::RemoveWire { wire_id }).unwrap();
    assert!(document.design.nets[&x].pins.contains(&bridge_far));
    apply(
        &mut document,
        OperationPayload::RemoveComponent {
            instance_id: bridge,
        },
    )
    .unwrap();
    assert!(!document.design.components.contains_key(&bridge));
    assert!(!document.design.nets[&x].pins.contains(&bridge_far));
}

fn run(
    document: &mut ProjectDocument,
    origin: OperationOrigin,
    defer_checks: bool,
    payload: OperationPayload,
) -> Result<(), OperationError> {
    let request = OperationRequest {
        operation_id: OperationId::new(),
        expected_revision: document.revision,
        origin,
        rationale: None,
        allow_invalid_placement: true,
        defer_checks,
        payload,
    };
    apply_operation(document, request).map(|_| ())
}

fn codes(document: &ProjectDocument) -> Vec<&str> {
    document
        .validation
        .iter()
        .map(|item| item.code.0.as_str())
        .collect()
}

/// Batch edit lets a person pass through a short; it stays listed as an error,
/// the project still saves and opens, and live edits that add nothing new
/// still go through.
#[test]
fn a_batch_edit_may_leave_a_short_that_is_reported_not_refused() {
    let Fixture {
        mut document,
        near,
        x,
        y,
        ..
    } = fixture();
    let short = OperationPayload::AssignPin {
        net_id: y,
        pin: near.clone(),
    };
    run(&mut document, OperationOrigin::User, true, short).unwrap();
    assert!(codes(&document).contains(&"circuit.bridge_short"));

    // An unrelated live edit: the short was already there, so it does not block.
    run(
        &mut document,
        OperationOrigin::User,
        false,
        OperationPayload::SetNetNotes {
            net_id: x,
            notes: "rework in progress".into(),
        },
    )
    .unwrap();

    let reopened = ProjectDocument::from_json(&to_snapshot_json(&document).unwrap()).unwrap();
    assert!(codes(&reopened).contains(&"circuit.bridge_short"));

    // Repairing it live is fine too.
    run(
        &mut document,
        OperationOrigin::User,
        false,
        OperationPayload::UnassignPin {
            net_id: y,
            pin: near,
        },
    )
    .unwrap();
    assert!(!codes(&document).contains(&"circuit.bridge_short"));
}

#[test]
fn agents_may_not_defer_checks() {
    let Fixture {
        mut document, x, ..
    } = fixture();
    let notes = OperationPayload::SetNetNotes {
        net_id: x,
        notes: String::new(),
    };
    let error = run(&mut document, OperationOrigin::Mcp, true, notes).unwrap_err();
    assert!(matches!(error, OperationError::DeferredChecksNotAllowed));

    let mut session = ProjectSession::new(document);
    let revision = session.document().revision;
    let error = session
        .apply_transaction(TransactionRequest {
            operation_id: OperationId::new(),
            expected_revision: revision,
            origin: OperationOrigin::Mcp,
            rationale: None,
            allow_invalid_placement: false,
            defer_checks: true,
            operations: vec![OperationPayload::SetNetNotes {
                net_id: x,
                notes: String::new(),
            }],
        })
        .unwrap_err();
    assert!(error.to_string().contains("only available"), "{error}");
}

/// An agent's transaction is checked once, at the end: its steps may pass
/// through a short as long as the result has none.
#[test]
fn a_transaction_is_checked_only_at_its_end() {
    let Fixture {
        document, near, y, ..
    } = fixture();
    let mut session = ProjectSession::new(document);
    let transaction = |session: &ProjectSession, operations| TransactionRequest {
        operation_id: OperationId::new(),
        expected_revision: session.document().revision,
        origin: OperationOrigin::Mcp,
        rationale: None,
        allow_invalid_placement: false,
        defer_checks: false,
        operations,
    };
    let through_a_short = vec![
        OperationPayload::AssignPin {
            net_id: y,
            pin: near.clone(),
        },
        OperationPayload::UnassignPin {
            net_id: y,
            pin: near.clone(),
        },
    ];
    let request = transaction(&session, through_a_short);
    session.apply_transaction(request).unwrap();

    let ending_in_a_short = vec![OperationPayload::AssignPin {
        net_id: y,
        pin: near,
    }];
    let request = transaction(&session, ending_in_a_short);
    let error = session.apply_transaction(request).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("short"),
        "{error}"
    );
}
