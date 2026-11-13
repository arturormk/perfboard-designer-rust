use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ComponentKind,
    ConnectionState, ElectricalNet, HolePos, NetId, OperationId, PinRef, ProjectId, Rotation, Side,
};
use perfboard_project::{
    Board, NetConnectivity, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument,
    apply_operation, net_connectivity, render_design_svg, to_snapshot_json,
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

const HEADER: &str = "schema_version=1\nid='header'\ndefinition_version='1'\nname='Header'\nallowed_rotations=['deg0']\n[body]\nx=-1.27\ny=-1.27\nwidth=5.08\nheight=2.54\n[[pins]]\nid='pin_1'\nnumber='1'\nname='Pin 1'\nx=0\ny=0\n[[pins]]\nid='pin_2'\nnumber='2'\nname='Pin 2'\nx=1\ny=0";

fn jumper_toml() -> String {
    HEADER.replacen("schema_version=1\n", "schema_version=1\nkind='jumper'\n", 1)
}

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
                name: "JP1".into(),
                position: HolePos::new(x, 1),
                rotation: Rotation::Deg0,
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

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "jumpers",
        Board::new("board", BoardSize::new(8, 4).unwrap(), 2.54),
    )
}

fn incomplete(document: &ProjectDocument) -> bool {
    document
        .validation
        .iter()
        .any(|diagnostic| diagnostic.code.0 == "circuit.net_incomplete")
}

fn shunt(
    document: &mut ProjectDocument,
    instance_id: ComponentInstanceId,
    shunted: bool,
) -> Result<(), String> {
    apply(
        document,
        OperationPayload::SetJumperShunt {
            instance_id,
            shunted,
        },
    )
}

#[test]
fn a_fitted_shunt_completes_the_net_across_the_jumper() {
    let mut document = document();
    let jumper = place(&mut document, &jumper_toml(), 2);
    net(
        &mut document,
        "SENSE",
        &[pin(jumper, "pin_1"), pin(jumper, "pin_2")],
    );
    assert!(incomplete(&document), "an open jumper leaves the net split");

    shunt(&mut document, jumper, true).unwrap();
    assert!(document.design.components[&jumper].shunted);
    assert!(!incomplete(&document), "the shunt joins the two pins");

    shunt(&mut document, jumper, false).unwrap();
    assert!(
        incomplete(&document),
        "removing the shunt splits the net again"
    );
}

#[test]
fn only_jumpers_take_a_shunt() {
    let mut document = document();
    let header = place(&mut document, HEADER, 2);
    let error = shunt(&mut document, header, true).unwrap_err();
    assert!(error.contains("not a two-pin jumper"), "{error}");
}

fn joins(document: &ProjectDocument) -> Vec<String> {
    document
        .validation
        .iter()
        .filter(|diagnostic| diagnostic.code.0 == "circuit.shunt_joins_nets")
        .map(|diagnostic| diagnostic.message.clone())
        .collect()
}

fn connectivity_of(document: &ProjectDocument, net_id: NetId) -> NetConnectivity {
    net_connectivity(&document.design)
        .into_iter()
        .find(|item| item.net_id == net_id)
        .unwrap()
}

#[test]
fn a_shunt_may_join_two_nets() {
    let mut document = document();
    let jumper = place(&mut document, &jumper_toml(), 2);
    let a = net(&mut document, "A", &[pin(jumper, "pin_1")]);
    let b = net(&mut document, "B", &[pin(jumper, "pin_2")]);

    shunt(&mut document, jumper, true).unwrap();
    assert_eq!(joins(&document), ["Shunt on JP1 joins nets `A` and `B`"]);
    let view = connectivity_of(&document, a);
    assert_eq!(view.joined_nets, [b]);
    assert_eq!(view.shunts, [jumper]);
    assert_eq!(connectivity_of(&document, b).joined_nets, [a]);
    assert_eq!(document.design.nets.len(), 2, "native nets stay separate");

    shunt(&mut document, jumper, false).unwrap();
    assert!(
        joins(&document).is_empty(),
        "removing the shunt splits them again"
    );
    assert_eq!(
        connectivity_of(&document, a).joined_nets,
        Vec::<NetId>::new()
    );
}

#[test]
fn shunts_join_nets_transitively_but_completeness_stays_native() {
    let mut document = document();
    let first = place(&mut document, &jumper_toml(), 1);
    let second = place(&mut document, &jumper_toml(), 4);
    let a = net(&mut document, "A", &[pin(first, "pin_1")]);
    let b = net(
        &mut document,
        "B",
        &[pin(first, "pin_2"), pin(second, "pin_1")],
    );
    let c = net(&mut document, "C", &[pin(second, "pin_2")]);
    shunt(&mut document, first, true).unwrap();
    shunt(&mut document, second, true).unwrap();

    let view = connectivity_of(&document, a);
    let mut expected = vec![b, c];
    expected.sort();
    assert_eq!(view.joined_nets, expected);
    assert_eq!(view.shunts.len(), 2);
    // B's two pins are joined only through other nets, never by its own
    // copper, so it is still reported as unrouted.
    assert!(incomplete(&document));
}

#[test]
fn update_metadata_turns_a_plain_header_into_a_jumper() {
    let mut document = document();
    let header = place(&mut document, HEADER, 2);
    let library = ComponentDefinition::from_toml(&jumper_toml()).unwrap();
    apply(
        &mut document,
        OperationPayload::UpdateComponentMetadata {
            instance_id: header,
            definition: library,
        },
    )
    .unwrap();
    assert_eq!(
        document.design.component_definitions["header"].kind,
        ComponentKind::Jumper
    );
    shunt(&mut document, header, true).unwrap();

    // Going back to a plain part drops the shunt with the jumper kind.
    apply(
        &mut document,
        OperationPayload::UpdateComponentMetadata {
            instance_id: header,
            definition: ComponentDefinition::from_toml(HEADER).unwrap(),
        },
    )
    .unwrap();
    assert_eq!(
        document.design.component_definitions["header"].kind,
        ComponentKind::Standard
    );
    assert!(!document.design.components[&header].shunted);
}

#[test]
fn update_metadata_keeps_the_kind_when_pins_differ() {
    let mut document = document();
    let header = place(&mut document, HEADER, 2);
    let mut library = ComponentDefinition::from_toml(&jumper_toml()).unwrap();
    library.pins[1].x = 2;
    apply(
        &mut document,
        OperationPayload::UpdateComponentMetadata {
            instance_id: header,
            definition: library,
        },
    )
    .unwrap();
    assert_eq!(
        document.design.component_definitions["header"].kind,
        ComponentKind::Standard
    );
}

#[test]
fn the_shunt_round_trips_and_renders() {
    let mut document = document();
    let jumper = place(&mut document, &jumper_toml(), 2);
    let json = to_snapshot_json(&document).unwrap();
    assert!(!json.contains("shunted"), "an open jumper writes no field");

    shunt(&mut document, jumper, true).unwrap();
    let json = to_snapshot_json(&document).unwrap();
    assert!(json.contains("\"shunted\": true"));
    let loaded = ProjectDocument::from_json(&json).unwrap();
    assert!(loaded.design.components[&jumper].shunted);
    assert!(render_design_svg(&loaded, Side::Component).contains(r#"class="shunt""#));
}
