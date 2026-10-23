use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    ComponentDefinition, ComponentInstance, ComponentInstanceId, ConnectionState, Diagnostic,
    DiagnosticSeverity, ElectricalNet, HolePos, Junction, JunctionId, NetId, OperationId, PinRef,
    Rotation, RoutePos, Side, Wire, WireColor, WireId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DocumentError, DocumentRevision, ProjectDocument, WorkingChange};

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
    DefineNet {
        net: ElectricalNet,
    },
    RemoveNet {
        net_id: NetId,
    },
    AssignPin {
        net_id: NetId,
        pin: PinRef,
    },
    UnassignPin {
        net_id: NetId,
        pin: PinRef,
    },
    AcceptNetProposal {
        net_id: NetId,
    },
    RejectNetProposal {
        net_id: NetId,
    },
    AddJunction {
        junction: Junction,
    },
    RemoveJunction {
        junction_id: JunctionId,
    },
    AddWire {
        wire: Wire,
    },
    RemoveWire {
        wire_id: WireId,
    },
    ModifyWirePath {
        wire_id: WireId,
        vertices: Vec<RoutePos>,
    },
    SetWireColor {
        wire_id: WireId,
        color: WireColor,
    },
    SetWireSide {
        wire_id: WireId,
        side: Side,
    },
    SetWireLock {
        wire_id: WireId,
        locked: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationResult {
    pub operation_id: OperationId,
    pub revision: DocumentRevision,
    pub normalized_payload: OperationPayload,
    pub affected_component_ids: BTreeSet<ComponentInstanceId>,
    pub affected_net_ids: BTreeSet<NetId>,
    pub affected_wire_ids: BTreeSet<WireId>,
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
    #[error("net already exists")]
    DuplicateNet,
    #[error("net was not found")]
    NetNotFound,
    #[error("wire already exists")]
    DuplicateWire,
    #[error("wire was not found")]
    WireNotFound,
    #[error("junction already exists")]
    DuplicateJunction,
    #[error("junction was not found")]
    JunctionNotFound,
    #[error("object is still referenced by circuit or routing data")]
    ObjectInUse,
    #[error("only proposed nets can be accepted or rejected")]
    NotAProposal,
    #[error(transparent)]
    InvalidDocument(#[from] DocumentError),
}

/// Applies one mutation atomically after checking its expected revision.
///
/// # Errors
/// Returns a typed conflict or invariant error without changing `document`.
#[allow(clippy::too_many_lines, clippy::similar_names)] // The match is the auditable operation registry.
pub fn apply_operation(
    document: &mut ProjectDocument,
    request: OperationRequest,
) -> Result<OperationResult, OperationError> {
    let before = document.design.clone();
    let before_validation = document.validation.clone();
    let metadata = request.clone();
    let result = apply_operation_untracked(document, request)?;
    if document.working_base.is_none() {
        document.working_base = Some(before.clone());
    }
    document.undo_stack.push(WorkingChange {
        operation_id: metadata.operation_id,
        origin: metadata.origin,
        rationale: metadata.rationale,
        summary: result.change_summary.clone(),
        payloads: vec![result.normalized_payload.clone()],
        before,
        after: document.design.clone(),
        before_validation,
        after_validation: document.validation.clone(),
    });
    document.redo_stack.clear();
    Ok(result)
}

#[allow(clippy::too_many_lines, clippy::similar_names)] // This match is the auditable operation registry.
pub(crate) fn apply_operation_untracked(
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
    let mut affected_nets = BTreeSet::new();
    let mut affected_wires = BTreeSet::new();
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
            if next
                .design
                .nets
                .values()
                .any(|net| net.pins.iter().any(|pin| pin.component_id == *instance_id))
            {
                return Err(OperationError::ObjectInUse);
            }
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
        OperationPayload::DefineNet { net } => {
            if next.design.nets.contains_key(&net.id) {
                return Err(OperationError::DuplicateNet);
            }
            next.design.nets.insert(net.id, net.clone());
            affected_nets.insert(net.id);
            format!("Defined net {}", net.name)
        }
        OperationPayload::RemoveNet { net_id } => {
            if next
                .design
                .wires
                .values()
                .any(|wire| wire.net_id == *net_id)
                || next
                    .design
                    .junctions
                    .values()
                    .any(|junction| junction.net_id == *net_id)
            {
                return Err(OperationError::ObjectInUse);
            }
            let net = next
                .design
                .nets
                .remove(net_id)
                .ok_or(OperationError::NetNotFound)?;
            affected_nets.insert(*net_id);
            format!("Removed net {}", net.name)
        }
        OperationPayload::AssignPin { net_id, pin } => {
            let net = next
                .design
                .nets
                .get_mut(net_id)
                .ok_or(OperationError::NetNotFound)?;
            net.pins.insert(pin.clone());
            affected_nets.insert(*net_id);
            format!("Assigned pin to net {}", net.name)
        }
        OperationPayload::UnassignPin { net_id, pin } => {
            if next.design.wires.values().any(|wire| {
                wire.net_id == *net_id
                    && [wire.start.clone(), wire.end.clone()].contains(&pin.clone().into())
            }) {
                return Err(OperationError::ObjectInUse);
            }
            let net = next
                .design
                .nets
                .get_mut(net_id)
                .ok_or(OperationError::NetNotFound)?;
            net.pins.remove(pin);
            affected_nets.insert(*net_id);
            format!("Unassigned pin from net {}", net.name)
        }
        OperationPayload::AcceptNetProposal { net_id } => {
            let net = next
                .design
                .nets
                .get_mut(net_id)
                .ok_or(OperationError::NetNotFound)?;
            if net.state != ConnectionState::Proposed {
                return Err(OperationError::NotAProposal);
            }
            net.state = ConnectionState::Accepted;
            affected_nets.insert(*net_id);
            format!("Accepted net proposal {}", net.name)
        }
        OperationPayload::RejectNetProposal { net_id } => {
            let net = next
                .design
                .nets
                .get(net_id)
                .ok_or(OperationError::NetNotFound)?;
            if net.state != ConnectionState::Proposed {
                return Err(OperationError::NotAProposal);
            }
            if next
                .design
                .wires
                .values()
                .any(|wire| wire.net_id == *net_id)
            {
                return Err(OperationError::ObjectInUse);
            }
            let name = net.name.clone();
            next.design
                .junctions
                .retain(|_, junction| junction.net_id != *net_id);
            next.design.nets.remove(net_id);
            affected_nets.insert(*net_id);
            format!("Rejected net proposal {name}")
        }
        OperationPayload::AddJunction { junction } => {
            if next
                .design
                .junctions
                .insert(junction.id, junction.clone())
                .is_some()
            {
                return Err(OperationError::DuplicateJunction);
            }
            affected_nets.insert(junction.net_id);
            format!("Added junction {}", junction.id)
        }
        OperationPayload::RemoveJunction { junction_id } => {
            if next.design.wires.values().any(|wire| matches!(&wire.start, perfboard_core::TerminalRef::Junction { junction_id: id } if id == junction_id) || matches!(&wire.end, perfboard_core::TerminalRef::Junction { junction_id: id } if id == junction_id)) { return Err(OperationError::ObjectInUse); }
            let junction = next
                .design
                .junctions
                .remove(junction_id)
                .ok_or(OperationError::JunctionNotFound)?;
            affected_nets.insert(junction.net_id);
            format!("Removed junction {junction_id}")
        }
        OperationPayload::AddWire { wire } => {
            if next.design.wires.contains_key(&wire.id) {
                return Err(OperationError::DuplicateWire);
            }
            next.design.wires.insert(wire.id, wire.clone());
            affected_nets.insert(wire.net_id);
            affected_wires.insert(wire.id);
            format!("Added wire {}", wire.id)
        }
        OperationPayload::RemoveWire { wire_id } => {
            let wire = next
                .design
                .wires
                .remove(wire_id)
                .ok_or(OperationError::WireNotFound)?;
            affected_nets.insert(wire.net_id);
            affected_wires.insert(*wire_id);
            format!("Removed wire {wire_id}")
        }
        OperationPayload::ModifyWirePath { wire_id, vertices } => {
            let wire = next
                .design
                .wires
                .get_mut(wire_id)
                .ok_or(OperationError::WireNotFound)?;
            wire.vertices.clone_from(vertices);
            affected_nets.insert(wire.net_id);
            affected_wires.insert(*wire_id);
            format!("Edited wire {wire_id} path")
        }
        OperationPayload::SetWireColor { wire_id, color } => {
            let wire = next
                .design
                .wires
                .get_mut(wire_id)
                .ok_or(OperationError::WireNotFound)?;
            wire.color.clone_from(color);
            affected_wires.insert(*wire_id);
            format!("Changed wire {wire_id} color")
        }
        OperationPayload::SetWireSide { wire_id, side } => {
            let wire = next
                .design
                .wires
                .get_mut(wire_id)
                .ok_or(OperationError::WireNotFound)?;
            wire.side = *side;
            affected_wires.insert(*wire_id);
            format!("Changed wire {wire_id} side")
        }
        OperationPayload::SetWireLock { wire_id, locked } => {
            let wire = next
                .design
                .wires
                .get_mut(wire_id)
                .ok_or(OperationError::WireNotFound)?;
            wire.locked = *locked;
            affected_wires.insert(*wire_id);
            format!(
                "{} wire {wire_id}",
                if *locked { "Locked" } else { "Unlocked" }
            )
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
        affected_net_ids: affected_nets,
        affected_wire_ids: affected_wires,
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
