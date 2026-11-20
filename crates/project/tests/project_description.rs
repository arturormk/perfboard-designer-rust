use perfboard_core::{BoardSize, ProjectId};
use perfboard_project::{Board, DocumentRevision, ProjectDocument, RevisionError};

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Fuzz pedal",
        Board::new("10x10 perfboard", BoardSize::new(10, 10).unwrap(), 2.54),
    )
}

#[test]
fn a_new_project_has_no_description() {
    assert_eq!(document().description, "");
}

#[test]
fn setting_the_description_trims_it_and_advances_the_revision() {
    let mut document = document();
    let before = document.revision;
    document
        .set_description(before, "\n  A two-transistor fuzz.\nRuns on 9 V.  \n")
        .unwrap();
    assert_eq!(document.description, "A two-transistor fuzz.\nRuns on 9 V.");
    assert_eq!(document.revision, DocumentRevision(before.0 + 1));
    assert!(
        document.undo_stack.is_empty(),
        "a description edit is not an undo step"
    );
    document.validate().unwrap();
}

#[test]
fn an_unchanged_description_leaves_the_revision_alone() {
    let mut document = document();
    document.set_description(document.revision, "Fuzz").unwrap();
    let revision = document.revision;
    document.set_description(revision, " Fuzz ").unwrap();
    assert_eq!(document.revision, revision);
}

#[test]
fn an_empty_description_clears_it() {
    let mut document = document();
    document.set_description(document.revision, "Fuzz").unwrap();
    document.set_description(document.revision, "   ").unwrap();
    assert_eq!(document.description, "");
}

#[test]
fn a_stale_revision_is_rejected() {
    let mut document = document();
    let revision = document.revision;
    assert!(matches!(
        document.set_description(DocumentRevision(revision.0 + 5), "Fuzz"),
        Err(RevisionError::StaleRevision { .. })
    ));
    assert_eq!(document.description, "");
}

#[test]
fn the_description_is_saved_only_when_written() {
    let mut document = document();
    let blank = serde_json::to_value(&document).unwrap();
    assert!(blank.get("description").is_none());

    document
        .set_description(document.revision, "A two-transistor fuzz.")
        .unwrap();
    let json = serde_json::to_string(&document).unwrap();
    let loaded: ProjectDocument = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.description, "A two-transistor fuzz.");

    let old: ProjectDocument = serde_json::from_value(blank).unwrap();
    assert_eq!(old.description, "");
}
