use perfboard_core::{BoardSize, ProjectId};
use perfboard_project::{Board, DocumentRevision, ProjectDocument, RevisionError};

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Untitled project",
        Board::new("10x10 perfboard", BoardSize::new(10, 10).unwrap(), 2.54),
    )
}

#[test]
fn renaming_trims_the_name_and_advances_the_revision() {
    let mut document = document();
    let before = document.revision;
    document.rename(before, "  Fuzz pedal ").unwrap();
    assert_eq!(document.name, "Fuzz pedal");
    assert_eq!(document.revision, DocumentRevision(before.0 + 1));
    assert!(
        document.undo_stack.is_empty(),
        "a rename is not an undo step"
    );
    document.validate().unwrap();
}

#[test]
fn renaming_to_the_same_name_changes_nothing() {
    let mut document = document();
    let before = document.revision;
    document.rename(before, "Untitled project").unwrap();
    assert_eq!(document.revision, before);
}

#[test]
fn renaming_rejects_blank_names_and_stale_revisions() {
    let mut document = document();
    let revision = document.revision;
    assert_eq!(
        document.rename(revision, "   "),
        Err(RevisionError::BlankProjectName)
    );
    assert!(matches!(
        document.rename(DocumentRevision(revision.0 + 5), "Other"),
        Err(RevisionError::StaleRevision { .. })
    ));
    assert_eq!(document.name, "Untitled project");
    assert_eq!(document.revision, revision);
}
