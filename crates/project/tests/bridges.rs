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
                component_side: false,
                band_count: perfboard_core::BandCount::Five,
                pin_labels: BTreeMap::new(),
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

fn place_bridge(document: &mut ProjectDocument, x: i32) -> ComponentInstanceId {
    let id = place(document, BRIDGE, x);
    apply(
        document,
        OperationPayload::SetBridgeSide {
            instance_id: id,
            side: Side::Component,
        },
    )
    .unwrap();
    id
}

/// A pin at hole 0 whose lead runs over holes 1 and 2 on the component side
/// (two component-side bridges) into a solder bridge from hole 2 to hole 3,
/// where another pin sits.
#[test]
fn component_side_bridges_are_made_with_the_owning_parts_lead() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "lead bridges",
        Board::new("board", BoardSize::new(8, 4).unwrap(), 2.54),
    );
    let owner = place(&mut document, PIN, 0);
    let first = place_bridge(&mut document, 0);
    let second = place_bridge(&mut document, 1);
    let solder = place(&mut document, BRIDGE, 2);
    let far = place(&mut document, PIN, 3);
    net(&mut document, "X", &[pin(owner, "p"), pin(far, "p")]);
    assert!(
        document.design.components[&first].component_side,
        "the operation moves the bridge to the component side"
    );

    // The bridges are copper: the net is complete without a wire.
    let connectivity = perfboard_project::net_connectivity(&document.design);
    assert_eq!(connectivity[0].islands.len(), 1, "{connectivity:?}");

    let plan = perfboard_project::assembly_plan(&document.design, document.revision);
    let step_of = |id: ComponentInstanceId| {
        plan.steps
            .iter()
            .position(|step| {
                step.objects
                    .iter()
                    .any(|object| object.id == id.to_string())
            })
            .unwrap()
    };
    // No step of their own: they are part of the pin's step.
    assert_eq!(step_of(first), step_of(owner));
    assert_eq!(step_of(second), step_of(owner));
    let owner_step = &plan.steps[step_of(owner)];
    let text = owner_step.instructions.join(" ");
    assert!(text.contains("over (1, 0)"), "{text}");
    assert!(text.contains("through (2, 0)"), "{text}");
    assert!(text.contains("lead's end at (2, 0)"), "{text}");
    // The solder bridge joins the lead's end, so it comes after.
    assert!(step_of(solder) > step_of(owner));
    assert!(
        plan.steps[step_of(solder)]
            .after
            .iter()
            .any(|dependency| dependency.step_id == owner_step.id),
        "{:?}",
        plan.steps[step_of(solder)].after
    );
    assert!(
        plan.warnings
            .iter()
            .all(|w| w.code != "assembly.lead_bridge_unattached")
    );
}

#[test]
fn a_component_side_bridge_with_no_lead_gets_its_own_step_and_a_warning() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "lone lead bridge",
        Board::new("board", BoardSize::new(8, 4).unwrap(), 2.54),
    );
    let lone = place_bridge(&mut document, 4);
    let plan = perfboard_project::assembly_plan(&document.design, document.revision);
    let step = plan
        .steps
        .iter()
        .find(|step| {
            step.objects
                .iter()
                .any(|object| object.id == lone.to_string())
        })
        .unwrap();
    assert_eq!(step.side, Side::Component);
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.code == "assembly.lead_bridge_unattached")
    );
}

#[test]
fn only_bridges_change_side() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "not a bridge",
        Board::new("board", BoardSize::new(8, 4).unwrap(), 2.54),
    );
    let part = place(&mut document, PIN, 0);
    let error = apply(
        &mut document,
        OperationPayload::SetBridgeSide {
            instance_id: part,
            side: Side::Component,
        },
    )
    .unwrap_err();
    assert!(error.contains("not a bridge"), "{error}");
}
