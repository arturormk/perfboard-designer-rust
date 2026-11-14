//! The `component_paths` project setting (ADR 0014) is optional, round-trips
//! unchanged, and is independent of the session's file path.

use std::path::{Path, PathBuf};

use perfboard_core::{BoardSize, ProjectId};
use perfboard_project::{Board, ProjectDocument, ProjectSession, RevisionError, SessionError};
use serde_json::Value;

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Paths",
        Board::new("board", BoardSize::new(10, 10).unwrap(), 2.54),
    )
}

#[test]
fn absent_paths_are_not_written_and_read_back_as_none() {
    let document = document();
    let json = serde_json::to_value(&document).unwrap();
    assert!(json.get("component_paths").is_none());
    let read: ProjectDocument = serde_json::from_value(json).unwrap();
    assert_eq!(read.component_paths, None);
}

#[test]
fn listed_paths_round_trip_including_an_empty_list() {
    for paths in [
        vec![],
        vec![
            "${PROJECT_DIR}/${PROJECT_STEM}.perfboard".to_owned(),
            "${USER_COMPONENTS}".to_owned(),
        ],
    ] {
        let mut document = document();
        document.component_paths = Some(paths.clone());
        let json = serde_json::to_value(&document).unwrap();
        assert_eq!(
            json["component_paths"],
            Value::from(paths.clone()),
            "{paths:?}"
        );
        let read: ProjectDocument = serde_json::from_value(json).unwrap();
        assert_eq!(read, document);
    }
}

#[test]
fn malformed_paths_still_load() {
    // Path rules are enforced when the library is resolved, so a bad entry
    // never makes a project unopenable.
    let mut json = serde_json::to_value(document()).unwrap();
    json["component_paths"] = Value::from(vec!["/etc", "${PROJECT_DIR}/../x"]);
    let read: ProjectDocument = serde_json::from_value(json).unwrap();
    assert_eq!(read.component_paths.unwrap().len(), 2);
}

#[test]
fn the_session_tracks_the_project_file() {
    let mut session = ProjectSession::new(document());
    assert_eq!(session.file_path(), None);

    session.replace_with_file(document(), Some(PathBuf::from("/a/b.perfboard.json")));
    assert_eq!(session.file_path(), Some(Path::new("/a/b.perfboard.json")));

    session.set_file_path(PathBuf::from("/c/d.perfboard.json"));
    assert_eq!(session.file_path(), Some(Path::new("/c/d.perfboard.json")));

    session.replace(document());
    assert_eq!(session.file_path(), None);
}

#[test]
fn editing_the_paths_checks_them_and_bumps_the_revision() {
    let mut session = ProjectSession::new(document());
    let start = session.document().revision;
    let paths = vec![
        " ${PROJECT_DIR}/parts ".to_owned(),
        "${USER_COMPONENTS}".to_owned(),
    ];
    let event = session.set_component_paths(start, Some(paths)).unwrap();
    assert_eq!(event.revision.0, start.0 + 1);
    assert_eq!(
        session.document().component_paths,
        Some(vec![
            "${PROJECT_DIR}/parts".to_owned(),
            "${USER_COMPONENTS}".to_owned()
        ])
    );

    // The same list again changes nothing.
    let revision = session.document().revision;
    let same = session.document().component_paths.clone();
    session.set_component_paths(revision, same).unwrap();
    assert_eq!(session.document().revision, revision);

    // Back to the default.
    session.set_component_paths(revision, None).unwrap();
    assert_eq!(session.document().component_paths, None);
    assert_eq!(session.document().revision.0, revision.0 + 1);
}

#[test]
fn invalid_or_stale_edits_are_refused() {
    let mut session = ProjectSession::new(document());
    let revision = session.document().revision;
    let error = session
        .set_component_paths(revision, Some(vec!["${PROJECT_DIR}/../x".into()]))
        .unwrap_err();
    assert!(matches!(
        error,
        SessionError::Revision(RevisionError::InvalidComponentPath { .. })
    ));
    assert_eq!(session.document().revision, revision);
    assert_eq!(session.document().component_paths, None);

    let stale = perfboard_project::DocumentRevision(revision.0 + 5);
    assert!(matches!(
        session.set_component_paths(stale, None),
        Err(SessionError::Revision(RevisionError::StaleRevision { .. }))
    ));
}
