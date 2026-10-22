use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    ComponentDefinition, ComponentInstance, ComponentInstanceId, Diagnostic, DiagnosticSeverity,
    HolePos, OperationId, Rotation,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DocumentError, DocumentRevision, ProjectDocument};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOrigin {
    User,
    Mcp,
    Import,
    SystemDerived,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationRequest {
    pub operation_id: OperationId,
    pub expected_revision: DocumentRevision,
    pub origin: OperationOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Accept placement diagnostics while preserving structural invariants.
    #[serde(default)]
    pub allow_invalid_placement: bool,
    #[serde(flatten)]
    pub payload: OperationPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // Serialized DTO clarity is preferable to boxed fields.
pub enum OperationPayload {
    AddComponent {
        definition: ComponentDefinition,
        instance: ComponentInstance,
    },
    RemoveComponent {
        instance_id: ComponentInstanceId,
    },
    MoveComponent {
        instance_id: ComponentInstanceId,
        position: HolePos,
    },
    RotateComponent {
        instance_id: ComponentInstanceId,
        rotation: Rotation,
    },
    ChangeInstanceProperties {
        instance_id: ComponentInstanceId,
        name: String,
        properties: BTreeMap<String, String>,
        notes: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationResult {
    pub operation_id: OperationId,
    pub revision: DocumentRevision,
    pub normalized_payload: OperationPayload,
    pub affected_component_ids: BTreeSet<ComponentInstanceId>,
    pub validation: Vec<Diagnostic>,
    pub change_summary: String,
}

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("expected revision {expected:?}, but current revision is {actual:?}")]
    StaleRevision {
        expected: DocumentRevision,
        actual: DocumentRevision,
    },
    #[error("component instance already exists")]
    DuplicateInstance,
    #[error("component definition id conflicts with its stored snapshot")]
    DefinitionConflict,
    #[error("component instance refers to a different definition")]
    DefinitionMismatch,
    #[error("component instance was not found")]
    InstanceNotFound,
    #[error("component definition does not allow the requested rotation")]
    RotationNotAllowed,
    #[error("operation would create an invalid placement")]
    InvalidPlacement { diagnostics: Vec<Diagnostic> },
    #[error(transparent)]
    InvalidDocument(#[from] DocumentError),
}

/// Applies one mutation atomically after checking its expected revision.
///
/// # Errors
/// Returns a typed conflict or invariant error without changing `document`.
#[allow(clippy::too_many_lines)] // The match is the auditable registry of Milestone 1 mutations.
pub fn apply_operation(
    document: &mut ProjectDocument,
    request: OperationRequest,
) -> Result<OperationResult, OperationError> {
    if request.expected_revision != document.revision {
        return Err(OperationError::StaleRevision {
            expected: request.expected_revision,
            actual: document.revision,
        });
    }
    let mut next = document.clone();
    let mut affected = BTreeSet::new();
    let summary = match &request.payload {
        OperationPayload::AddComponent {
            definition,
            instance,
        } => {
            if instance.definition_id != definition.id {
                return Err(OperationError::DefinitionMismatch);
            }
            if next.design.components.contains_key(&instance.id) {
                return Err(OperationError::DuplicateInstance);
            }
            if let Some(existing) = next.design.component_definitions.get(&definition.id) {
                if existing != definition {
                    return Err(OperationError::DefinitionConflict);
                }
            }
            definition
                .validate()
                .map_err(|error| DocumentError::InvalidComponentDefinition {
                    id: definition.id.clone(),
                    message: error.to_string(),
                })?;
            if !definition.allowed_rotations.contains(&instance.rotation) {
                return Err(OperationError::RotationNotAllowed);
            }
            next.design
                .component_definitions
                .insert(definition.id.clone(), definition.clone());
            next.design.components.insert(instance.id, instance.clone());
            affected.insert(instance.id);
            format!("Added {}", instance.name)
        }
        OperationPayload::RemoveComponent { instance_id } => {
            let removed = next
                .design
                .components
                .remove(instance_id)
                .ok_or(OperationError::InstanceNotFound)?;
            affected.insert(*instance_id);
            format!("Removed {}", removed.name)
        }
        OperationPayload::MoveComponent {
            instance_id,
            position,
        } => {
            let instance = next
                .design
                .components
                .get_mut(instance_id)
                .ok_or(OperationError::InstanceNotFound)?;
            instance.position = *position;
            affected.insert(*instance_id);
            format!("Moved {}", instance.name)
        }
        OperationPayload::RotateComponent {
            instance_id,
            rotation,
        } => {
            let instance = next
                .design
                .components
                .get_mut(instance_id)
                .ok_or(OperationError::InstanceNotFound)?;
            let definition = next
                .design
                .component_definitions
                .get(&instance.definition_id)
                .ok_or(OperationError::InstanceNotFound)?;
            if !definition.allowed_rotations.contains(rotation) {
                return Err(OperationError::RotationNotAllowed);
            }
            instance.rotation = *rotation;
            affected.insert(*instance_id);
            format!("Rotated {}", instance.name)
        }
        OperationPayload::ChangeInstanceProperties {
            instance_id,
            name,
            properties,
            notes,
        } => {
            let instance = next
                .design
                .components
                .get_mut(instance_id)
                .ok_or(OperationError::InstanceNotFound)?;
            instance.name.clone_from(name);
            instance.properties.clone_from(properties);
            instance.notes.clone_from(notes);
            affected.insert(*instance_id);
            format!("Updated {} properties", instance.name)
        }
    };
    next.validation = next.calculate_validation();
    if !request.allow_invalid_placement
        && next
            .validation
            .iter()
            .any(|item| item.severity == DiagnosticSeverity::Error)
    {
        return Err(OperationError::InvalidPlacement {
            diagnostics: next.validation,
        });
    }
    next.revision.0 += 1;
    next.validate()?;
    *document = next;
    Ok(OperationResult {
        operation_id: request.operation_id,
        revision: document.revision,
        normalized_payload: request.payload,
        affected_component_ids: affected,
        validation: document.validation.clone(),
        change_summary: summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Board;
    use perfboard_core::{BoardSize, ProjectId};
    use uuid::Uuid;

    #[test]
    fn stale_operation_does_not_change_document() {
        let mut document = ProjectDocument::new(
            ProjectId::from_uuid(Uuid::nil()),
            "x",
            Board::new("x", BoardSize::new(2, 2).unwrap(), 2.54),
        );
        let before = document.clone();
        let request = OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: DocumentRevision(1),
            origin: OperationOrigin::User,
            rationale: None,
            allow_invalid_placement: false,
            payload: OperationPayload::RemoveComponent {
                instance_id: ComponentInstanceId::new(),
            },
        };
        assert!(matches!(
            apply_operation(&mut document, request),
            Err(OperationError::StaleRevision { .. })
        ));
        assert_eq!(document, before);
    }
}
