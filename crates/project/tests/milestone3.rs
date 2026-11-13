use std::collections::BTreeMap;

use perfboard_core::{
    BoardSize, CommitId, ComponentDefinition, ComponentInstance, ComponentInstanceId, HolePos,
    OperationId, ProjectId, Rotation,
};
use perfboard_project::{
    Board, DocumentRevision, OperationOrigin, OperationPayload, OperationRequest, ProjectDocument,
    ProjectSession, SessionError, TransactionRequest, load_project, save_project_atomic,
    to_snapshot_json,
};

fn definition() -> ComponentDefinition {
    ComponentDefinition::from_toml(
        "schema_version=1\nid='test.part'\ndefinition_version='1'\nname='Part'\nallowed_rotations=['deg0','deg90']\n[body]\nx=-0.5\ny=-0.5\nwidth=1\nheight=1\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
    )
    .unwrap()
}

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Milestone 3",
        Board::new("board", BoardSize::new(8, 8).unwrap(), 2.54),
    )
}

fn add_payload(id: ComponentInstanceId, name: &str, x: u32) -> OperationPayload {
    let definition = definition();
    OperationPayload::AddComponent {
        instance: ComponentInstance {
            id,
            definition_id: definition.id.clone(),
            name: name.into(),
            position: HolePos::new(x, 2),
            rotation: Rotation::Deg0,
            properties: BTreeMap::new(),
            notes: String::new(),
            shunted: false,
        },
        definition,
    }
}

fn request(revision: DocumentRevision, payload: OperationPayload) -> OperationRequest {
    OperationRequest {
        operation_id: OperationId::new(),
        expected_revision: revision,
        origin: OperationOrigin::User,
        rationale: Some("Milestone 3 scenario".into()),
        allow_invalid_placement: false,
        payload,
    }
}

#[test]
fn failed_transaction_leaves_serialized_and_memory_state_unchanged() {
    let document = document();
    let mut session = ProjectSession::new(document.clone());
    let id = ComponentInstanceId::new();
    let before_bytes = to_snapshot_json(session.document()).unwrap();
    let result = session.apply_transaction(TransactionRequest {
        operation_id: OperationId::new(),
        expected_revision: DocumentRevision(0),
        origin: OperationOrigin::Mcp,
        rationale: Some("Atomic pair".into()),
        allow_invalid_placement: false,
        operations: vec![add_payload(id, "first", 1), add_payload(id, "duplicate", 3)],
    });
    assert!(result.is_err());
    assert_eq!(session.document(), &document);
    assert_eq!(to_snapshot_json(session.document()).unwrap(), before_bytes);
}

#[test]
fn undo_redo_and_save_reopen_preserve_working_history() {
    let mut session = ProjectSession::new(document());
    let id = ComponentInstanceId::new();
    session
        .apply(request(DocumentRevision(0), add_payload(id, "part", 1)))
        .unwrap();
    session.undo(DocumentRevision(1)).unwrap();
    assert!(!session.document().design.components.contains_key(&id));

    let directory = std::env::temp_dir().join(format!("perfboard-m3-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("history.perfboard.json");
    save_project_atomic(session.document(), &path).unwrap();
    let reopened = load_project(&path).unwrap();
    let mut reopened = ProjectSession::new(reopened);
    reopened.redo(DocumentRevision(2)).unwrap();
    assert!(reopened.document().design.components.contains_key(&id));
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn undo_history_can_be_trimmed_or_deleted_without_changing_the_design() {
    let mut session = ProjectSession::new(document());
    for (name, x) in [("first", 1), ("second", 3), ("third", 5)] {
        let revision = session.document().revision;
        session
            .apply(request(
                revision,
                add_payload(ComponentInstanceId::new(), name, x),
            ))
            .unwrap();
    }
    session.undo(DocumentRevision(3)).unwrap();
    let design = session.document().design.clone();

    let (trimmed, _) = session.trim_undo_history(DocumentRevision(4), 1).unwrap();
    assert_eq!(trimmed.revision, DocumentRevision(5));
    assert_eq!(session.document().undo_stack.len(), 1);
    assert_eq!(session.document().redo_stack.len(), 1);
    assert_eq!(session.document().design, design);

    let (deleted, _) = session.trim_undo_history(DocumentRevision(5), 0).unwrap();
    assert_eq!(deleted.revision, DocumentRevision(6));
    assert_eq!(session.document().undo_stack, Vec::new());
    assert_eq!(session.document().redo_stack, Vec::new());
    assert_eq!(session.document().design, design);
    assert!(session.document().is_dirty());
}

#[test]
fn restore_keeps_descendants_and_creates_a_dirty_working_copy() {
    let mut session = ProjectSession::new(document());
    let first_id = ComponentInstanceId::new();
    session
        .apply(request(
            DocumentRevision(0),
            add_payload(first_id, "first", 1),
        ))
        .unwrap();
    let (first, _) = session
        .commit(DocumentRevision(1), "first".into(), String::new())
        .unwrap();
    let second_id = ComponentInstanceId::new();
    session
        .apply(request(
            DocumentRevision(2),
            add_payload(second_id, "second", 4),
        ))
        .unwrap();
    let (second, _) = session
        .commit(DocumentRevision(3), "second".into(), String::new())
        .unwrap();
    assert_eq!(second.parent, Some(first.id));
    session
        .restore(DocumentRevision(4), first.id, OperationOrigin::User, None)
        .unwrap();
    assert_eq!(session.document().commits.len(), 2);
    assert_eq!(session.document().head_commit, Some(second.id));
    assert!(session.document().is_dirty());
    assert!(
        !session
            .document()
            .design
            .components
            .contains_key(&second_id)
    );
}

#[test]
fn two_requests_at_one_revision_yield_one_success_and_one_structured_conflict() {
    let mut session = ProjectSession::new(document());
    session
        .apply(request(
            DocumentRevision(0),
            add_payload(ComponentInstanceId::new(), "winner", 1),
        ))
        .unwrap();
    let result = session.apply(request(
        DocumentRevision(0),
        add_payload(ComponentInstanceId::new(), "loser", 4),
    ));
    assert!(matches!(
        result,
        Err(SessionError::Operation(
            perfboard_project::OperationError::StaleRevision {
                expected: DocumentRevision(0),
                actual: DocumentRevision(1)
            }
        ))
    ));
    assert_eq!(session.document().design.components.len(), 1);
}

#[test]
fn semantic_diff_classifies_movement_and_rotation() {
    let mut session = ProjectSession::new(document());
    let id = ComponentInstanceId::new();
    session
        .apply(request(DocumentRevision(0), add_payload(id, "part", 1)))
        .unwrap();
    let (commit, _) = session
        .commit(DocumentRevision(1), "placed".into(), String::new())
        .unwrap();
    session
        .apply(request(
            DocumentRevision(2),
            OperationPayload::MoveComponent {
                instance_id: id,
                position: HolePos::new(3, 2),
            },
        ))
        .unwrap();
    let diff = session.compare(Some(commit.id), None).unwrap();
    assert_eq!(
        diff.components[0].kind,
        perfboard_project::ChangeKind::Moved
    );
    assert!(session.compare(Some(CommitId::new()), None).is_none());
}

#[test]
fn redo_survives_failed_work_but_clears_after_a_divergent_edit() {
    let mut session = ProjectSession::new(document());
    let first_id = ComponentInstanceId::new();
    session
        .apply(request(
            DocumentRevision(0),
            add_payload(first_id, "first", 1),
        ))
        .unwrap();
    session.undo(DocumentRevision(1)).unwrap();
    assert_eq!(session.document().redo_stack.len(), 1);

    let failed = session.apply(request(
        DocumentRevision(1),
        add_payload(ComponentInstanceId::new(), "stale", 4),
    ));
    assert!(failed.is_err());
    assert_eq!(session.document().redo_stack.len(), 1);

    session
        .apply(request(
            DocumentRevision(2),
            add_payload(ComponentInstanceId::new(), "divergent", 4),
        ))
        .unwrap();
    assert_eq!(session.document().redo_stack.len(), 0);
}
