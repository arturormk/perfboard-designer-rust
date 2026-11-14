//! The `component_paths` project setting (ADR 0014) is optional, round-trips
//! unchanged, and is independent of the session's file path.

use std::path::{Path, PathBuf};

use perfboard_core::{BoardSize, ProjectId};
use perfboard_project::{Board, ProjectDocument, ProjectSession};
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
