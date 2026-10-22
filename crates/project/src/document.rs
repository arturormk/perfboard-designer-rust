use perfboard_core::{BoardSize, ProjectId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROJECT_SCHEMA_VERSION: SchemaVersion = SchemaVersion(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub u32);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentRevision(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectDocument {
    pub schema_version: SchemaVersion,
    pub project_id: ProjectId,
    pub name: String,
    pub revision: DocumentRevision,
    pub design: Design,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub board: Board,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub name: String,
    #[serde(flatten)]
    pub size: BoardSize,
    pub pitch_mm: f64,
}

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("unsupported project schema version {actual}; this build supports version {supported}")]
    UnsupportedSchema { actual: u32, supported: u32 },
    #[error("project name must not be blank")]
    BlankProjectName,
    #[error("board name must not be blank")]
    BlankBoardName,
    #[error("board pitch must be a positive, finite number")]
    InvalidPitch,
    #[error("invalid project JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

impl ProjectDocument {
    pub fn new(project_id: ProjectId, name: impl Into<String>, board: Board) -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            project_id,
            name: name.into(),
            revision: DocumentRevision::default(),
            design: Design { board },
        }
    }

    /// Validates the schema envelope and currently implemented domain invariants.
    ///
    /// # Errors
    ///
    /// Returns a specific [`DocumentError`] for the first invalid invariant.
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(DocumentError::UnsupportedSchema {
                actual: self.schema_version.0,
                supported: PROJECT_SCHEMA_VERSION.0,
            });
        }
        if self.name.trim().is_empty() {
            return Err(DocumentError::BlankProjectName);
        }
        self.design.board.validate()
    }

    /// Deserializes and validates a project document at the input boundary.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentError::InvalidJson`] for invalid JSON/domain encodings,
    /// or another [`DocumentError`] when a decoded invariant is invalid.
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let document: Self = serde_json::from_str(json)?;
        document.validate()?;
        Ok(document)
    }
}

impl Board {
    pub fn new(name: impl Into<String>, size: BoardSize, pitch_mm: f64) -> Self {
        Self {
            name: name.into(),
            size,
            pitch_mm,
        }
    }

    /// Validates board metadata and physical pitch.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentError::BlankBoardName`] or [`DocumentError::InvalidPitch`].
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.name.trim().is_empty() {
            return Err(DocumentError::BlankBoardName);
        }
        if !self.pitch_mm.is_finite() || self.pitch_mm <= 0.0 {
            return Err(DocumentError::InvalidPitch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use perfboard_core::BoardSize;
    use uuid::Uuid;

    use super::*;
    use crate::to_snapshot_json;

    fn empty_project() -> ProjectDocument {
        ProjectDocument::new(
            ProjectId::from_uuid(Uuid::parse_str("018f1000-0000-7000-8000-000000000001").unwrap()),
            "Empty fixture",
            Board::new("1x1 perfboard", BoardSize::new(1, 1).unwrap(), 2.54),
        )
    }

    #[test]
    fn empty_project_round_trips_without_loss() {
        let original = empty_project();
        let json = to_snapshot_json(&original).expect("serialize project");
        let decoded = ProjectDocument::from_json(&json).expect("deserialize project");

        assert_eq!(decoded, original);
        assert_eq!(to_snapshot_json(&decoded).unwrap(), json);
    }

    #[test]
    fn permanent_empty_fixture_matches_canonical_snapshot() {
        let fixture = include_str!("../../../tests/fixtures/project-v1/empty-1x1.perfboard.json");
        let decoded = ProjectDocument::from_json(fixture).expect("valid permanent fixture");

        assert_eq!(to_snapshot_json(&decoded).unwrap(), fixture);
    }

    #[test]
    fn unsupported_schema_is_rejected_at_boundary() {
        let mut value = serde_json::to_value(empty_project()).unwrap();
        value["schema_version"] = serde_json::json!(99);
        let json = serde_json::to_string(&value).unwrap();

        assert!(matches!(
            ProjectDocument::from_json(&json),
            Err(DocumentError::UnsupportedSchema { actual: 99, .. })
        ));
    }

    #[test]
    fn invalid_pitch_is_rejected_at_boundary() {
        let mut value = serde_json::to_value(empty_project()).unwrap();
        value["design"]["board"]["pitch_mm"] = serde_json::json!(0.0);

        assert!(matches!(
            ProjectDocument::from_json(&value.to_string()),
            Err(DocumentError::InvalidPitch)
        ));
    }

    #[test]
    fn zero_sized_board_is_rejected_during_deserialization() {
        let mut value = serde_json::to_value(empty_project()).unwrap();
        value["design"]["board"]["holes_x"] = serde_json::json!(0);

        assert!(matches!(
            ProjectDocument::from_json(&value.to_string()),
            Err(DocumentError::InvalidJson(_))
        ));
    }
}
