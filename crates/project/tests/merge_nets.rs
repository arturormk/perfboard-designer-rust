use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    Constraint, ConstraintId, ConstraintKind, ConstraintOrigin, ConstraintSeverity, ElectricalNet,
    HolePos, NetId, OperationId, PinRef, ProjectId, Rotation, RoutePos, Side, TerminalRef, Wire,
    WireColor, WireId,
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

const PIN: &str = "schema_version=1\nid='pin'\ndefinition_version='1'\nname='Pin'\nallowed_rotations=['deg0']\n[body]\nx=-0.2\ny=-0.2\nwidth=0.4\nheight=0.4\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0";

fn place(document: &mut ProjectDocument, x: u32) -> PinRef {
    let definition = ComponentDefinition::from_toml(PIN).unwrap();
    let id = ComponentInstanceId::new();
    apply(
        document,
        OperationPayload::AddComponent {
            definition: definition.clone(),
            instance: ComponentInstance {
                id,
                definition_id: definition.id.clone(),
                name: format!("P{x}"),
                position: HolePos::new(x, 1),
                rotation: Rotation::Deg0,
                properties: BTreeMap::new(),
                notes: String::new(),
                shunted: false,
            },
        },
    )
    .unwrap();
    PinRef {
        component_id: id,
        pin_id: "p".into(),
    }
}

fn net(
    document: &mut ProjectDocument,
    name: &str,
    state: ConnectionState,
    pins: &[PinRef],
    notes: &str,
) -> NetId {
    let id = NetId::new();
    apply(
        document,
        OperationPayload::DefineNet {
            net: ElectricalNet {
                id,
                name: name.into(),
                state,
                pins: pins.iter().cloned().collect::<BTreeSet<_>>(),
                notes: notes.into(),
            },
        },
    )
    .unwrap();
    id
}

fn wire(document: &mut ProjectDocument, net_id: NetId, from: &PinRef, to: &PinRef) -> WireId {
    let id = WireId::new();
    let x = |pin: &PinRef| {
        let instance = &document.design.components[&pin.component_id];
        i64::from(instance.position.x) * 2
    };
    let vertices = vec![RoutePos::new(x(from), 2), RoutePos::new(x(to), 2)];
    apply(
        document,
        OperationPayload::AddWire {
            wire: Wire {
                id,
                net_id,
                start: TerminalRef::from(from.clone()),
                end: TerminalRef::from(to.clone()),
                side: Side::Solder,
                color: WireColor::Green,
                vertices,
                locked: false,
                notes: String::new(),
            },
        },
    )
    .unwrap();
    id
}

fn merge(document: &mut ProjectDocument, source: NetId, target: NetId) -> Result<(), String> {
    apply(
        document,
        OperationPayload::MergeNets {
            source_net_id: source,
            target_net_id: target,
        },
    )
}

fn incomplete(document: &ProjectDocument) -> usize {
    document
        .validation
        .iter()
        .filter(|diagnostic| diagnostic.code.0 == "circuit.net_incomplete")
        .count()
}

#[test]
fn merging_moves_pins_wires_and_constraints_onto_the_target() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "merge",
        Board::new("board", BoardSize::new(12, 4).unwrap(), 2.54),
    );
    let p0 = place(&mut document, 0);
    let p3 = place(&mut document, 3);
    let p6 = place(&mut document, 6);
    let p9 = place(&mut document, 9);
    let sense = net(
        &mut document,
        "SENSE",
        ConnectionState::Accepted,
        &[p0.clone(), p3.clone()],
        "",
    );
    let ret = net(
        &mut document,
        "RETURN",
        ConnectionState::Accepted,
        &[p6.clone(), p9.clone()],
        "Back to the sensor",
    );
    let sense_wire = wire(&mut document, sense, &p0, &p3);
    let return_wire = wire(&mut document, ret, &p6, &p9);
    let constraint_id = ConstraintId::new();
    apply(
        &mut document,
        OperationPayload::AddConstraint {
            constraint: Constraint {
                id: constraint_id,
                severity: ConstraintSeverity::Preferred,
                origin: ConstraintOrigin::User,
                rationale: String::new(),
                annotation: String::new(),
                kind: ConstraintKind::WireSide {
                    net_id: ret,
                    side: Side::Solder,
                },
            },
        },
    )
    .unwrap();
    assert_eq!(incomplete(&document), 0);

    merge(&mut document, ret, sense).unwrap();

    let design = &document.design;
    assert!(!design.nets.contains_key(&ret), "the source net is removed");
    let merged = &design.nets[&sense];
    assert_eq!(merged.name, "SENSE");
    assert_eq!(merged.pins, BTreeSet::from([p0, p3, p6, p9.clone()]));
    assert_eq!(merged.notes, "Back to the sensor");
    assert_eq!(design.wires[&sense_wire].net_id, sense);
    assert_eq!(design.wires[&return_wire].net_id, sense);
    assert!(matches!(
        design.constraints[&constraint_id].kind,
        ConstraintKind::WireSide { net_id, .. } if net_id == sense
    ));
    // Two routed islands on one net: the merged net now needs a link.
    assert_eq!(incomplete(&document), 1);
    // The merged net behaves like any other: the RETURN wire can be extended.
    let p11 = place(&mut document, 11);
    apply(
        &mut document,
        OperationPayload::AssignPin {
            net_id: sense,
            pin: p11.clone(),
        },
    )
    .unwrap();
    wire(&mut document, sense, &p9, &p11);
}

#[test]
fn merging_rejects_self_missing_and_proposed_nets() {
    let mut document = ProjectDocument::new(
        ProjectId::new(),
        "merge",
        Board::new("board", BoardSize::new(8, 4).unwrap(), 2.54),
    );
    let p0 = place(&mut document, 0);
    let accepted = net(&mut document, "A", ConnectionState::Accepted, &[p0], "");
    let proposed = net(&mut document, "B", ConnectionState::Proposed, &[], "");

    let error = merge(&mut document, accepted, accepted).unwrap_err();
    assert!(error.contains("into itself"), "{error}");
    let error = merge(&mut document, NetId::new(), accepted).unwrap_err();
    assert!(error.contains("not found"), "{error}");
    let error = merge(&mut document, proposed, accepted).unwrap_err();
    assert!(error.contains("only accepted nets"), "{error}");
    let error = merge(&mut document, accepted, proposed).unwrap_err();
    assert!(error.contains("only accepted nets"), "{error}");
    assert_eq!(document.design.nets.len(), 2, "nothing changed");
}
