//! Parts may be parked off the board on any side (ADR 0021).

use std::collections::BTreeMap;

use perfboard_core::{
    BoardSize, ComponentInstance, ComponentInstanceId, HolePos, OperationId, ProjectId, Rotation,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument, apply_operation,
    to_snapshot_json,
};

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Off board",
        Board::new("board", BoardSize::new(25, 18).unwrap(), 2.54),
    )
}

fn request(document: &ProjectDocument, payload: OperationPayload) -> OperationRequest {
    OperationRequest {
        operation_id: OperationId::new(),
        expected_revision: document.revision,
        origin: OperationOrigin::User,
        rationale: None,
        allow_invalid_placement: true,
        defer_checks: false,
        payload,
    }
}

fn add(document: &mut ProjectDocument, position: HolePos) -> ComponentInstanceId {
    let example =
        ProjectDocument::from_json(include_str!("../../../examples/nano-rs485.perfboard.json"))
            .unwrap();
    let definition = example.design.component_definitions["local.nano-like"].clone();
    let id = ComponentInstanceId::new();
    let payload = OperationPayload::AddComponent {
        instance: ComponentInstance {
            id,
            definition_id: definition.id.clone(),
            name: String::new(),
            position,
            rotation: Rotation::Deg0,
            value: String::new(),
            properties: BTreeMap::new(),
            notes: String::new(),
            shunted: false,
        },
        definition,
    };
    let request = request(document, payload);
    let result = apply_operation(document, request).unwrap();
    assert!(
        result
            .validation
            .iter()
            .any(|item| item.code.0 == "placement.out_of_bounds")
    );
    id
}

#[test]
fn a_part_can_be_placed_left_of_and_above_the_board() {
    let mut document = document();
    let left = add(&mut document, HolePos::new(-12, 3));
    let above = add(&mut document, HolePos::new(4, -20));
    assert_eq!(
        document.design.components[&left].position,
        HolePos::new(-12, 3)
    );
    assert_eq!(
        document.design.components[&above].position,
        HolePos::new(4, -20)
    );

    let reopened = ProjectDocument::from_json(&to_snapshot_json(&document).unwrap()).unwrap();
    assert_eq!(
        reopened.design.components[&left].position,
        HolePos::new(-12, 3)
    );
}

#[test]
fn a_part_can_be_moved_from_off_board_onto_it() {
    let mut document = document();
    let id = add(&mut document, HolePos::new(-12, -2));
    let payload = OperationPayload::MoveComponent {
        instance_id: id,
        position: HolePos::new(2, 2),
    };
    let request = request(&document, payload);
    let result = apply_operation(&mut document, request).unwrap();
    assert!(
        !result
            .validation
            .iter()
            .any(|item| item.code.0 == "placement.out_of_bounds")
    );
}
