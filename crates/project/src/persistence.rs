use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use thiserror::Error;
use uuid::Uuid;

use crate::{DocumentError, ProjectDocument, to_snapshot_json};

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("project I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("project serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("project path must have a parent directory")]
    MissingParent,
}

/// Loads and validates a project file.
///
/// # Errors
/// Returns I/O, JSON, or domain validation failures.
pub fn load_project(path: impl AsRef<Path>) -> Result<ProjectDocument, PersistenceError> {
    let source = fs::read_to_string(path)?;
    Ok(ProjectDocument::from_json(&source)?)
}

/// Writes through a flushed sibling temporary file and atomically renames it.
///
/// # Errors
/// Returns I/O, serialization, or validation failures.
pub fn save_project_atomic(
    document: &ProjectDocument,
    path: impl AsRef<Path>,
) -> Result<(), PersistenceError> {
    document.validate()?;
    let path = path.as_ref();
    let parent = path.parent().ok_or(PersistenceError::MissingParent)?;
    let file_name = path
        .file_name()
        .ok_or(PersistenceError::MissingParent)?
        .to_string_lossy();
    let temporary: PathBuf = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(to_snapshot_json(document)?.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Board;
    use perfboard_core::{BoardSize, ProjectId};

    #[test]
    fn save_and_reopen_preserves_document() {
        let directory = std::env::temp_dir().join(format!("perfboard-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("project.perfboard.json");
        let document = ProjectDocument::new(
            ProjectId::new(),
            "Test",
            Board::new("Board", BoardSize::new(3, 2).unwrap(), 2.54),
        );
        save_project_atomic(&document, &path).unwrap();
        assert_eq!(load_project(&path).unwrap(), document);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
