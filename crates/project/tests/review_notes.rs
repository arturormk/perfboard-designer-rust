//! Review notes (ADR 0015) round-trip through the project file, stay out of
//! history, and can be written without an expected revision.

use perfboard_core::{BoardSize, ObjectReference, ProjectId};
use perfboard_project::{
    Board, NewReviewNote, OperationRequest, ProjectDocument, ProjectSession, ProjectView,
    ReviewNoteSeverity, ReviewNoteStatus, ReviewNoteUpdate,
};

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Notes",
        Board::new("board", BoardSize::new(10, 10).unwrap(), 2.54),
    )
}

fn note(severity: ReviewNoteSeverity, title: &str) -> NewReviewNote {
    NewReviewNote {
        severity,
        title: title.into(),
        detail: "Because.".into(),
        objects: vec![ObjectReference {
            kind: "component".into(),
            id: "c1".into(),
        }],
    }
}

#[test]
fn no_notes_are_not_written() {
    let json = serde_json::to_value(document()).unwrap();
    assert!(json.get("review_notes").is_none());
    let read: ProjectDocument = serde_json::from_value(json).unwrap();
    assert_eq!(read.review_notes, Vec::new());
}

#[test]
fn notes_round_trip_in_order() {
    let mut session = ProjectSession::new(document());
    session
        .add_review_notes(vec![
            note(ReviewNoteSeverity::Critical, "Swap A and B"),
            note(ReviewNoteSeverity::Suggestion, "Add a test point"),
        ])
        .unwrap();
    let json = serde_json::to_value(session.document()).unwrap();
    assert_eq!(json["review_notes"][0]["severity"], "critical");
    assert_eq!(json["review_notes"][1]["title"], "Add a test point");
    assert_eq!(json["review_notes"][0]["objects"][0]["kind"], "component");
    let read: ProjectDocument = serde_json::from_value(json).unwrap();
    assert_eq!(&read, session.document());
    read.validate().unwrap();
}

#[test]
fn notes_are_not_history_and_survive_undo() {
    let mut session = ProjectSession::new(document());
    let request: OperationRequest = serde_json::from_value(serde_json::json!({
        "operation_id": uuid::Uuid::new_v4(),
        "expected_revision": 0,
        "origin": "user",
        "type": "resize_board",
        "holes_x": 12,
        "holes_y": 10,
        "offset_x": 0,
        "offset_y": 0
    }))
    .unwrap();
    session.apply(request).unwrap();
    let (added, event) = session
        .add_review_notes(vec![note(ReviewNoteSeverity::Warning, "Too tight")])
        .unwrap();
    assert_eq!(event.kind, "review_notes");
    assert_eq!(event.summary, "Added review note “Too tight”");
    assert_eq!(session.document().undo_stack.len(), 1);

    session.undo(event.revision).unwrap();
    assert_eq!(session.document().review_notes, added);

    let view = ProjectView::from_document(session.document());
    assert_eq!(view.review_notes, added);
}

#[test]
fn note_writes_leave_the_revision_alone_but_count_as_unsaved() {
    let mut session = ProjectSession::new(document());
    let revision = session.document().revision;
    assert!(!session.has_unsaved_changes());

    let (added, event) = session
        .add_review_notes(vec![
            note(ReviewNoteSeverity::Critical, "a"),
            note(ReviewNoteSeverity::Info, "b"),
        ])
        .unwrap();
    assert_eq!(event.revision, revision);
    assert_eq!(added[0].revision, Some(revision));
    assert!(session.has_unsaved_changes());
    assert!(session.view().unsaved);

    session.mark_saved("/tmp/notes.perfboard.json".into());
    assert!(!session.has_unsaved_changes());

    session
        .move_review_note(added[1].id, Some(added[0].id))
        .unwrap();
    assert!(session.has_unsaved_changes());
    session.mark_saved("/tmp/notes.perfboard.json".into());

    // A move that changes nothing is not unsaved work.
    session
        .move_review_note(added[1].id, Some(added[0].id))
        .unwrap();
    assert!(!session.has_unsaved_changes());

    let (removed, _) = session.remove_review_notes(&[added[0].id]).unwrap();
    assert_eq!(removed[0].title, "a");
    assert!(session.has_unsaved_changes());
    session.insert_review_note(removed[0].clone(), 0).unwrap();
    assert_eq!(session.document().review_notes[0].title, "a");
    assert_eq!(session.document().revision, revision);
    assert_eq!(session.view().notes_generation, 4);
}

#[test]
fn design_edits_count_as_unsaved_and_opening_resets_the_baseline() {
    let mut session = ProjectSession::new(document());
    let request: OperationRequest = serde_json::from_value(serde_json::json!({
        "operation_id": uuid::Uuid::new_v4(),
        "expected_revision": 0,
        "origin": "user",
        "type": "resize_board",
        "holes_x": 12,
        "holes_y": 10,
        "offset_x": 0,
        "offset_y": 0
    }))
    .unwrap();
    session.apply(request).unwrap();
    assert!(session.has_unsaved_changes());
    session.replace_with_file(document(), Some("/tmp/other.perfboard.json".into()));
    assert!(!session.has_unsaved_changes());
}

#[test]
fn a_reply_keeps_the_revision_and_a_response_records_the_current_one() {
    let mut session = ProjectSession::new(document());
    let (added, _) = session
        .add_review_notes(vec![note(ReviewNoteSeverity::Warning, "Add a bulk cap")])
        .unwrap();
    let id = added[0].id;
    let request: OperationRequest = serde_json::from_value(serde_json::json!({
        "operation_id": uuid::Uuid::new_v4(),
        "expected_revision": 0,
        "origin": "user",
        "type": "resize_board",
        "holes_x": 12,
        "holes_y": 10,
        "offset_x": 0,
        "offset_y": 0
    }))
    .unwrap();
    session.apply(request).unwrap();
    session.mark_saved("/tmp/notes.perfboard.json".into());

    let (note, event) = session
        .update_review_note(
            id,
            ReviewNoteUpdate {
                status: Some(ReviewNoteStatus::ReviewRequested),
                reply: Some("I added one at the regulator.".into()),
                ..ReviewNoteUpdate::default()
            },
        )
        .unwrap();
    assert_eq!(event.summary, "Updated review note “Add a bulk cap”");
    assert_eq!(note.revision, added[0].revision, "a reply is not a review");
    assert!(session.has_unsaved_changes());
    assert!(session.document().undo_stack.len() == 1, "not an undo step");

    let (note, _) = session
        .update_review_note(
            id,
            ReviewNoteUpdate {
                status: Some(ReviewNoteStatus::Open),
                response: Some("Confirmed; it covers this.".into()),
                ..ReviewNoteUpdate::default()
            },
        )
        .unwrap();
    assert_eq!(note.revision, Some(session.document().revision));
    assert_eq!(note.reply, "I added one at the regulator.");

    let json = serde_json::to_value(session.document()).unwrap();
    let read: ProjectDocument = serde_json::from_value(json).unwrap();
    assert_eq!(read.review_notes[0], note);
}
