use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardFeatures, BoardSize, CommitId, ComponentDefinition, ComponentInstance,
    ComponentInstanceId, Diagnostic, ElectricalNet, Junction, JunctionId, NetId, ProjectId, Wire,
    WireId, validate_design_placements,
};
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub commits: BTreeMap<CommitId, crate::Commit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commit_order: Vec<CommitId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_commit: Option<CommitId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_base: Option<Design>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undo_stack: Vec<crate::WorkingChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redo_stack: Vec<crate::WorkingChange>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub board: Board,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub component_definitions: BTreeMap<String, ComponentDefinition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<ComponentInstanceId, ComponentInstance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nets: BTreeMap<NetId, ElectricalNet>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub junctions: BTreeMap<JunctionId, Junction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub wires: BTreeMap<WireId, Wire>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub name: String,
    #[serde(flatten)]
    pub size: BoardSize,
    pub pitch_mm: f64,
    #[serde(flatten, default, skip_serializing_if = "BoardFeatures::is_empty")]
    pub features: BoardFeatures,
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
    #[error("board margins and mounting holes must use finite, non-negative dimensions")]
    InvalidBoardFeatures,
    #[error("board feature lies outside the hole grid")]
    BoardFeatureOutOfBounds,
    #[error("invalid embedded component definition `{id}`: {message}")]
    InvalidComponentDefinition { id: String, message: String },
    #[error("component definition map key `{key}` does not match embedded id `{id}`")]
    ComponentDefinitionKeyMismatch { key: String, id: String },
    #[error("component instance map key does not match its embedded id")]
    ComponentInstanceKeyMismatch,
    #[error("component instance `{instance_id}` refers to missing definition `{definition_id}`")]
    MissingComponentDefinition {
        instance_id: String,
        definition_id: String,
    },
    #[error("component instance `{instance_id}` uses a rotation its definition does not allow")]
    InvalidInstanceRotation { instance_id: String },
    #[error("net map key does not match its embedded id")]
    NetKeyMismatch,
    #[error("junction map key does not match its embedded id")]
    JunctionKeyMismatch,
    #[error("wire map key does not match its embedded id")]
    WireKeyMismatch,
    #[error("invalid circuit or route: {0}")]
    InvalidCircuit(String),
    #[error("stored validation report is stale")]
    StaleValidation,
    #[error("invalid revision history: {0}")]
    InvalidRevisionHistory(String),
    #[error("invalid project JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

impl ProjectDocument {
    #[must_use]
    pub fn new(project_id: ProjectId, name: impl Into<String>, board: Board) -> Self {
        let design = Design {
            board,
            component_definitions: BTreeMap::new(),
            components: BTreeMap::new(),
            nets: BTreeMap::new(),
            junctions: BTreeMap::new(),
            wires: BTreeMap::new(),
        };
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            project_id,
            name: name.into(),
            revision: DocumentRevision::default(),
            working_base: None,
            design,
            validation: Vec::new(),
            commits: BTreeMap::new(),
            commit_order: Vec::new(),
            head_commit: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    #[must_use]
    pub fn calculate_validation(&self) -> Vec<Diagnostic> {
        let mut diagnostics = validate_design_placements(
            self.design.board.size,
            &self.design.board.features,
            &self.design.component_definitions,
            &self.design.components,
        );
        diagnostics.extend(crate::connectivity::diagnostics(&self.design));
        diagnostics.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then(left.message.cmp(&right.message))
        });
        diagnostics
    }

    /// Validates the schema envelope and all implemented domain invariants.
    ///
    /// # Errors
    /// Returns a specific [`DocumentError`] for the first invalid invariant.
    #[allow(clippy::too_many_lines)] // Keeping all deserialization-boundary checks together is auditable.
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
        self.design.board.validate()?;
        for (key, definition) in &self.design.component_definitions {
            if key != &definition.id {
                return Err(DocumentError::ComponentDefinitionKeyMismatch {
                    key: key.clone(),
                    id: definition.id.clone(),
                });
            }
            definition
                .validate()
                .map_err(|error| DocumentError::InvalidComponentDefinition {
                    id: key.clone(),
                    message: error.to_string(),
                })?;
        }
        if self
            .design
            .components
            .iter()
            .any(|(id, value)| id != &value.id)
        {
            return Err(DocumentError::ComponentInstanceKeyMismatch);
        }
        for (id, instance) in &self.design.components {
            let definition = self
                .design
                .component_definitions
                .get(&instance.definition_id)
                .ok_or_else(|| DocumentError::MissingComponentDefinition {
                    instance_id: id.to_string(),
                    definition_id: instance.definition_id.clone(),
                })?;
            if !definition.allowed_rotations.contains(&instance.rotation) {
                return Err(DocumentError::InvalidInstanceRotation {
                    instance_id: id.to_string(),
                });
            }
        }
        if self.design.nets.iter().any(|(id, value)| id != &value.id) {
            return Err(DocumentError::NetKeyMismatch);
        }
        if self
            .design
            .junctions
            .iter()
            .any(|(id, value)| id != &value.id)
        {
            return Err(DocumentError::JunctionKeyMismatch);
        }
        if self.design.wires.iter().any(|(id, value)| id != &value.id) {
            return Err(DocumentError::WireKeyMismatch);
        }
        crate::connectivity::validate_structure(&self.design)
            .map_err(DocumentError::InvalidCircuit)?;
        let calculated = self.calculate_validation();
        if self.validation != calculated {
            return Err(DocumentError::StaleValidation);
        }
        if self.commits.iter().any(|(id, commit)| id != &commit.id) {
            return Err(DocumentError::InvalidRevisionHistory(
                "commit map key does not match its embedded id".into(),
            ));
        }
        let ordered_commits: BTreeSet<_> = self.commit_order.iter().copied().collect();
        if self.commit_order.len() != self.commits.len()
            || ordered_commits.len() != self.commit_order.len()
            || self
                .commit_order
                .iter()
                .any(|id| !self.commits.contains_key(id))
        {
            return Err(DocumentError::InvalidRevisionHistory(
                "commit order does not contain every commit exactly once".into(),
            ));
        }
        if self
            .head_commit
            .is_some_and(|id| !self.commits.contains_key(&id))
        {
            return Err(DocumentError::InvalidRevisionHistory(
                "head commit does not exist".into(),
            ));
        }
        let validate_snapshot = |design: &Design,
                                 expected_validation: Option<&Vec<Diagnostic>>|
         -> Result<(), DocumentError> {
            let mut snapshot = Self {
                schema_version: self.schema_version,
                project_id: self.project_id,
                name: self.name.clone(),
                revision: self.revision,
                design: design.clone(),
                validation: Vec::new(),
                commits: BTreeMap::new(),
                commit_order: Vec::new(),
                head_commit: None,
                working_base: None,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
            };
            snapshot.validation = snapshot.calculate_validation();
            if expected_validation.is_some_and(|expected| expected != &snapshot.validation) {
                return Err(DocumentError::StaleValidation);
            }
            snapshot.validate()
        };
        if let Some(base) = &self.working_base {
            validate_snapshot(base, None)?;
        }
        for commit in self.commits.values() {
            validate_snapshot(&commit.design, Some(&commit.validation))?;
        }
        for change in self.undo_stack.iter().chain(&self.redo_stack) {
            validate_snapshot(&change.before, Some(&change.before_validation))?;
            validate_snapshot(&change.after, Some(&change.after_validation))?;
        }
        Ok(())
    }

    /// Deserializes and validates a project document at the input boundary.
    ///
    /// # Errors
    /// Returns a JSON/domain error or an invariant error.
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let document: Self = serde_json::from_str(json)?;
        document.validate()?;
        Ok(document)
    }
}

impl Board {
    #[must_use]
    pub fn new(name: impl Into<String>, size: BoardSize, pitch_mm: f64) -> Self {
        Self {
            name: name.into(),
            size,
            pitch_mm,
            features: BoardFeatures::default(),
        }
    }

    /// Validates board metadata and physical geometry.
    ///
    /// # Errors
    /// Returns a board invariant error.
    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.name.trim().is_empty() {
            return Err(DocumentError::BlankBoardName);
        }
        if !self.pitch_mm.is_finite() || self.pitch_mm <= 0.0 {
            return Err(DocumentError::InvalidPitch);
        }
        if !self.features.margins_mm.is_valid()
            || self
                .features
                .mounting_holes
                .iter()
                .any(|hole| !hole.diameter_mm.is_finite() || hole.diameter_mm <= 0.0)
        {
            return Err(DocumentError::InvalidBoardFeatures);
        }
        let inside = |position: perfboard_core::HolePos| {
            position.x < self.size.holes_x() && position.y < self.size.holes_y()
        };
        if self
            .features
            .mounting_holes
            .iter()
            .any(|hole| !inside(hole.position))
            || self.features.forbidden_areas.iter().any(|area| {
                !inside(area.min)
                    || !inside(area.max)
                    || area.min.x > area.max.x
                    || area.min.y > area.max.y
            })
        {
            return Err(DocumentError::BoardFeatureOutOfBounds);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_snapshot_json;
    use perfboard_core::BoardSize;
    use uuid::Uuid;

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
        let json = to_snapshot_json(&original).unwrap();
        assert_eq!(ProjectDocument::from_json(&json).unwrap(), original);
    }

    #[test]
    fn permanent_empty_fixture_matches_canonical_snapshot() {
        let fixture = include_str!("../../../tests/fixtures/project-v1/empty-1x1.perfboard.json");
        let decoded = ProjectDocument::from_json(fixture).unwrap();
        assert_eq!(to_snapshot_json(&decoded).unwrap(), fixture);
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
}
