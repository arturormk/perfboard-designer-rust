use std::collections::BTreeMap;

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, HolePos, OperationId,
    ProjectId, Rotation,
};
use perfboard_project::{
    Board, DocumentRevision, OperationError, OperationOrigin, OperationPayload, OperationRequest,
    ProjectDocument, apply_operation, load_project, save_project_atomic,
};
use uuid::Uuid;

fn apply(document: &mut ProjectDocument, payload: OperationPayload) {
    let request = OperationRequest {
        operation_id: OperationId::new(),
        expected_revision: document.revision,
        origin: OperationOrigin::User,
        rationale: Some("Milestone 1 demonstration".into()),
        payload,
    };
    apply_operation(document, request).expect("valid demonstration operation");
}

#[test]
#[allow(clippy::too_many_lines)] // The full vertical slice is intentionally one scenario.
fn place_move_rotate_save_and_reopen_example_components() {
    let nano = ComponentDefinition::from_toml(include_str!(
        "../../../components/arduino-nano-like/component.toml"
    ))
    .unwrap();
    let rs485 = ComponentDefinition::from_toml(include_str!(
        "../../../components/rs485-breakout/component.toml"
    ))
    .unwrap();
    let nano_id = ComponentInstanceId::from_uuid(
        Uuid::parse_str("018f1000-0000-7000-8000-000000000101").unwrap(),
    );
    let rs485_id = ComponentInstanceId::from_uuid(
        Uuid::parse_str("018f1000-0000-7000-8000-000000000102").unwrap(),
    );
    let mut document = ProjectDocument::new(
        ProjectId::from_uuid(Uuid::parse_str("018f1000-0000-7000-8000-000000000100").unwrap()),
        "Nano RS-485 controller",
        Board::new(
            "25x18 isolated-pad perfboard",
            BoardSize::new(25, 18).unwrap(),
            2.54,
        ),
    );

    apply(
        &mut document,
        OperationPayload::AddComponent {
            definition: nano,
            instance: ComponentInstance {
                id: nano_id,
                definition_id: "dev.perfboard.nano-like".into(),
                name: "Controller".into(),
                position: HolePos::new(2, 2),
                rotation: Rotation::Deg0,
                properties: BTreeMap::from([("socketed".into(), "true".into())]),
                notes: "USB faces the board edge".into(),
            },
        },
    );
    apply(
        &mut document,
        OperationPayload::MoveComponent {
            instance_id: nano_id,
            position: HolePos::new(17, 2),
        },
    );
    apply(
        &mut document,
        OperationPayload::RotateComponent {
            instance_id: nano_id,
            rotation: Rotation::Deg90,
        },
    );
    apply(
        &mut document,
        OperationPayload::AddComponent {
            definition: rs485,
            instance: ComponentInstance {
                id: rs485_id,
                definition_id: "dev.perfboard.rs485-breakout".into(),
                name: "RS-485 transceiver".into(),
                position: HolePos::new(16, 12),
                rotation: Rotation::Deg0,
                properties: BTreeMap::new(),
                notes: "Keep terminal reachable".into(),
            },
        },
    );
    apply(
        &mut document,
        OperationPayload::MoveComponent {
            instance_id: rs485_id,
            position: HolePos::new(16, 14),
        },
    );
    apply(
        &mut document,
        OperationPayload::RotateComponent {
            instance_id: rs485_id,
            rotation: Rotation::Deg180,
        },
    );

    assert_eq!(document.revision, DocumentRevision(6));
    assert_eq!(document.validation, Vec::new());
    let example = include_str!("../../../examples/nano-rs485.perfboard.json");
    assert_eq!(ProjectDocument::from_json(example).unwrap(), document);

    let before_invalid_move = document.clone();
    let invalid_move = OperationRequest {
        operation_id: OperationId::new(),
        expected_revision: document.revision,
        origin: OperationOrigin::User,
        rationale: None,
        payload: OperationPayload::MoveComponent {
            instance_id: nano_id,
            position: HolePos::new(0, 0),
        },
    };
    let error = apply_operation(&mut document, invalid_move).unwrap_err();
    let OperationError::InvalidPlacement { diagnostics } = error else {
        panic!("expected a placement error");
    };
    assert!(
        diagnostics
            .iter()
            .any(|item| item.code.0 == "placement.out_of_bounds")
    );
    assert_eq!(document, before_invalid_move);

    let path = std::env::temp_dir().join(format!("milestone1-{}.perfboard.json", Uuid::new_v4()));
    save_project_atomic(&document, &path).unwrap();
    let reopened = load_project(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(reopened, document);
}
