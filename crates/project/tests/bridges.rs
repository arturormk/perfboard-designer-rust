use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState,
    ElectricalNet, HolePos, NetId, OperationId, PinRef, ProjectId, Rotation, RoutePos, Side,
    TerminalRef, Wire, WireColor, WireId,
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
const BRIDGE: &str = "schema_version=1\nkind='bridge'\nid='bridge'\ndefinition_version='1'\nname='Bridge'\nallowed_rotations=['deg0']\n[body]\nx=0\ny=-0.2\nwidth=2.54\nheight=0.4\n[[pins]]\nid='a'\nnumber='A'\nname='A'\nx=0\ny=0\n[[pins]]\nid='b'\nnumber='B'\nname='B'\nx=1\ny=0";

fn place(document: &mut ProjectDocument, toml: &str, x: u32) -> ComponentInstanceId {
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
                properties: BTreeMap::new(),
                notes: String::new(),
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
    assert!(error.contains("bridge"), "{error}");
}
