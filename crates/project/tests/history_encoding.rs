//! Serialized history (ADR 0013): undo/redo snapshots are stored as JSON
//! patches, commits record operations without snapshots, and version 1 files
//! with full snapshots still load.

use std::collections::BTreeMap;

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, HolePos, OperationId,
    ProjectId, Rotation,
};
use perfboard_project::{
    Board, OperationOrigin, OperationPayload, OperationRequest, PROJECT_SCHEMA_VERSION,
    ProjectDocument, ProjectSession, to_snapshot_json, undo_history_bytes,
};
use serde_json::{Value, json};

fn definition() -> ComponentDefinition {
    ComponentDefinition::from_toml(
        "schema_version=1\nid='test.part'\ndefinition_version='1'\nname='Part'\nallowed_rotations=['deg0','deg90']\n[body]\nx=-0.5\ny=-0.5\nwidth=1\nheight=1\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0",
    )
    .unwrap()
}

fn add(session: &mut ProjectSession, name: &str, x: u32, y: u32) {
    let definition = definition();
    session
        .apply(OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: session.document().revision,
            origin: OperationOrigin::Mcp,
            rationale: Some(format!("Place {name}")),
            allow_invalid_placement: false,
            payload: OperationPayload::AddComponent {
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
                definition,
            },
        })
        .unwrap();
}

/// A commit, further edits, and one undone edit so both stacks and the
/// working base are populated.
fn session_with_history(edits: u32) -> ProjectSession {
    let mut session = ProjectSession::new(ProjectDocument::new(
        ProjectId::new(),
        "History",
        Board::new("board", BoardSize::new(40, 40).unwrap(), 2.54),
    ));
    add(&mut session, "first", 1, 1);
    add(&mut session, "second", 3, 1);
    let revision = session.document().revision;
    session
        .commit(revision, "Two parts".into(), String::new())
        .unwrap();
    for index in 0..edits {
        add(
            &mut session,
            &format!("part {index}"),
            1 + 2 * (index % 19),
            4 + 2 * (index / 19),
        );
    }
    let revision = session.document().revision;
    session.undo(revision).unwrap();
    session
}

/// The version 1 shape of `document`: full snapshots and validation reports
/// in every undo/redo entry and in every commit change.
fn version_1_json(document: &ProjectDocument) -> String {
    let mut value = serde_json::to_value(document).unwrap();
    let object = value.as_object_mut().unwrap();
    object.insert("schema_version".into(), json!(1));
    object.remove("working_base_patch");
    if let Some(base) = &document.working_base {
        object.insert("working_base".into(), serde_json::to_value(base).unwrap());
    }
    object.insert(
        "undo_stack".into(),
        serde_json::to_value(&document.undo_stack).unwrap(),
    );
    object.insert(
        "redo_stack".into(),
        serde_json::to_value(&document.redo_stack).unwrap(),
    );
    for commit in object["commits"].as_object_mut().unwrap().values_mut() {
        commit["changes"] = serde_json::to_value(&document.undo_stack).unwrap();
    }
    serde_json::to_string_pretty(&value).unwrap()
}

#[test]
fn history_round_trips_exactly() {
    let session = session_with_history(5);
    let document = session.document();
    assert_eq!(document.undo_stack.len(), 4);
    assert_eq!(document.redo_stack.len(), 1);
    assert!(document.working_base.is_some());

    let json = to_snapshot_json(document).unwrap();
    assert_eq!(&ProjectDocument::from_json(&json).unwrap(), document);
}

#[test]
fn history_is_stored_as_patches_without_snapshots() {
    let session = session_with_history(5);
    let value: Value =
        serde_json::from_str(&to_snapshot_json(session.document()).unwrap()).unwrap();

    assert_eq!(value["schema_version"], json!(PROJECT_SCHEMA_VERSION.0));
    assert!(value.get("working_base").is_none());
    assert!(
        value["working_base_patch"]
            .as_array()
            .is_some_and(|patch| !patch.is_empty())
    );
    for entry in value["undo_stack"]
        .as_array()
        .unwrap()
        .iter()
        .chain(value["redo_stack"].as_array().unwrap())
    {
        for field in ["before", "after", "before_validation", "after_validation"] {
            assert!(
                entry.get(field).is_none(),
                "history entry carries `{field}`"
            );
        }
    }
    // The history is contiguous, so each entry only needs the patch that
    // crosses its own change.
    for entry in value["undo_stack"].as_array().unwrap() {
        assert!(entry.get("after_patch").is_none());
        assert!(
            entry["before_patch"]
                .as_array()
                .is_some_and(|patch| !patch.is_empty())
        );
    }
    for entry in value["redo_stack"].as_array().unwrap() {
        assert!(entry.get("before_patch").is_none());
        assert!(
            entry["after_patch"]
                .as_array()
                .is_some_and(|patch| !patch.is_empty())
        );
    }
    let commit = value["commits"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert!(commit["design"].is_object());
    let change = &commit["changes"][0];
    assert_eq!(change["rationale"], json!("Place first"));
    assert!(change.get("before").is_none() && change.get("after").is_none());
}

#[test]
fn non_contiguous_history_still_round_trips() {
    let session = session_with_history(5);
    let mut document = session.document().clone();
    // Dropping a middle entry breaks the before/after chain.
    document.undo_stack.remove(1);

    let json = to_snapshot_json(&document).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert!(value["undo_stack"][0].get("after_patch").is_some());
    assert_eq!(ProjectDocument::from_json(&json).unwrap(), document);
}

#[test]
fn version_1_history_is_upgraded_on_load() {
    let session = session_with_history(5);
    let document = session.document();

    let upgraded = ProjectDocument::from_json(&version_1_json(document)).unwrap();
    assert_eq!(upgraded.schema_version, PROJECT_SCHEMA_VERSION);
    assert_eq!(upgraded.undo_stack, document.undo_stack);
    assert_eq!(upgraded.redo_stack, document.redo_stack);
    assert_eq!(upgraded.working_base, document.working_base);
    let commit = upgraded.commits.values().next().unwrap();
    assert_eq!(commit.changes.len(), document.undo_stack.len());
    assert_eq!(
        to_snapshot_json(
            &ProjectDocument::from_json(&to_snapshot_json(&upgraded).unwrap()).unwrap()
        )
        .unwrap(),
        to_snapshot_json(&upgraded).unwrap()
    );
}

#[test]
fn patch_encoding_keeps_long_histories_small() {
    let session = session_with_history(40);
    let document = session.document();
    let v1 = version_1_json(document).len();
    let v2 = to_snapshot_json(document).unwrap().len();
    assert!(v2 * 10 < v1, "v2 is {v2} bytes, v1 is {v1} bytes");
    assert!(undo_history_bytes(document) < v2);
}
