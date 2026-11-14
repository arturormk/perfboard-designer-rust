use std::collections::{BTreeMap, BTreeSet};

use perfboard_core::{
    BoardSize, ComponentDefinition, ComponentInstance, ComponentInstanceId, ComponentKind,
    ConnectionState, Constraint, ConstraintId, ConstraintKind, Diagnostic, DiagnosticSeverity,
    ElectricalNet, HolePos, JumperInventoryEntry, Junction, JunctionId, NetId, OperationId, PinRef,
    Rail, RailId, Rotation, RoutePos, RouteProposal, Saddle, SaddleId, Side, Wire, WireColor,
    WireId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DocumentError, DocumentRevision, ProjectDocument, WorkingChange};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationOrigin {
    User,
    Mcp,
    Import,
    SystemDerived,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WireEndpoint {
    Start,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // Serialized DTO clarity is preferable to boxed fields.
pub enum OperationPayload {
    ResizeBoard {
        holes_x: u32,
        holes_y: u32,
        offset_x: i64,
        offset_y: i64,
    },
    AddComponent {
        definition: ComponentDefinition,
        instance: ComponentInstance,
    },
    UpdateComponentMetadata {
        instance_id: ComponentInstanceId,
        definition: ComponentDefinition,
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
    /// Fits (`shunted: true`) or removes the shunt on a two-pin jumper. A
    /// shunted jumper joins its two pins; when they are on two different nets
    /// it joins those nets while fitted (reported as an advisory).
    SetJumperShunt {
        instance_id: ComponentInstanceId,
        shunted: bool,
    },
    DefineNet {
        net: ElectricalNet,
    },
    RemoveNet {
        net_id: NetId,
    },
    /// Changes a net's name. The name must be non-empty and not used by
    /// another net (compared case-insensitively).
    RenameNet {
        net_id: NetId,
        name: String,
    },
    /// Moves everything on `source_net_id` (pins, wires, rails, junctions
    /// and net constraints) onto `target_net_id`, then removes the source.
    /// The target keeps its name. Both nets must be accepted.
    MergeNets {
        source_net_id: NetId,
        target_net_id: NetId,
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
    AddRail {
        rail: Rail,
    },
    RemoveRail {
        rail_id: RailId,
    },
    ModifyRailPath {
        rail_id: RailId,
        vertices: Vec<RoutePos>,
    },
    /// Changes a rail's insulation color. Allowed on locked rails, like wires.
    SetRailColor {
        rail_id: RailId,
        color: WireColor,
    },
    /// Adds a net-free wire tie between two free holes.
    AddSaddle {
        saddle: Saddle,
    },
    /// Replaces a saddle's holes, side, color or notes.
    UpdateSaddle {
        saddle: Saddle,
    },
    RemoveSaddle {
        saddle_id: SaddleId,
    },
    RemoveWire {
        wire_id: WireId,
    },
    ModifyWirePath {
        wire_id: WireId,
        vertices: Vec<RoutePos>,
    },
    ReconnectWireEndpoint {
        wire_id: WireId,
        endpoint: WireEndpoint,
        terminal: perfboard_core::TerminalRef,
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
    AddConstraint {
        constraint: Constraint,
    },
    UpdateConstraint {
        constraint: Constraint,
    },
    RemoveConstraint {
        constraint_id: ConstraintId,
    },
    UpsertJumperInventory {
        entry: JumperInventoryEntry,
    },
    RemoveJumperInventory {
        inventory_id: String,
    },
    AcceptRouteProposal {
        proposal: RouteProposal,
        wire_id: WireId,
        color: WireColor,
        #[serde(default)]
        notes: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replace_wire_id: Option<WireId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_route_over: Option<bool>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OperationResult {
    pub operation_id: OperationId,
    pub revision: DocumentRevision,
    pub normalized_payload: OperationPayload,
    pub affected_component_ids: BTreeSet<ComponentInstanceId>,
    pub affected_net_ids: BTreeSet<NetId>,
    pub affected_wire_ids: BTreeSet<WireId>,
    pub affected_constraint_ids: BTreeSet<ConstraintId>,
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
    #[error("board dimensions must contain at least one hole on each axis")]
    InvalidBoardSize,
    #[error("board resize offset must keep either the near or far edge anchored")]
    InvalidBoardResizeOffset,
    #[error("board resize would move existing content outside the coordinate range")]
    BoardResizeOutOfRange,
    #[error("net already exists")]
    DuplicateNet,
    #[error("net was not found")]
    NetNotFound,
    #[error("net name must not be empty")]
    EmptyNetName,
    #[error("another net is already named {0}")]
    NetNameTaken(String),
    #[error("wire already exists")]
    DuplicateWire,
    #[error("wire was not found")]
    WireNotFound,
    #[error("rail already exists")]
    DuplicateRail,
    #[error("rail was not found")]
    RailNotFound,
    #[error("rail must contain at least two vertices and no zero-length segments")]
    UnsupportedRailRoute,
    #[error("wire must contain at least two vertices and no zero-length segments")]
    UnsupportedWireRoute,
    #[error("wire route passes through an unrelated component pin")]
    WireCrossesPin,
    #[error("rail route passes through a component pin")]
    RailCrossesPin,
    #[error("wire endpoint must refer to an existing terminal on the wire's net")]
    InvalidWireTerminal,
    #[error("wire endpoint pin is assigned to a different net")]
    PinAssignedToDifferentNet,
    #[error("junction already exists")]
    DuplicateJunction,
    #[error("junction was not found")]
    JunctionNotFound,
    #[error("constraint already exists")]
    DuplicateConstraint,
    #[error("constraint was not found")]
    ConstraintNotFound,
    #[error("a net cannot be merged into itself")]
    MergeIntoSelf,
    #[error("only accepted nets can be merged; accept or reject the proposal first")]
    MergeProposedNet,
    #[error("component is not a two-pin jumper")]
    NotAJumper,
    #[error("jumper inventory entry was not found")]
    JumperInventoryNotFound,
    #[error("locked wire requires an explicit route replacement operation")]
    LockedWire,
    #[error("locked rail cannot be changed")]
    LockedRail,
    #[error("saddle already exists")]
    DuplicateSaddle,
    #[error("saddle was not found")]
    SaddleNotFound,
    #[error("saddle needs two different holes on the board")]
    InvalidSaddle,
    #[error("saddle hole is not free: {0}")]
    SaddleHoleOccupied(String),
    #[error("saddle passes over a component pin")]
    SaddleCrossesPin,
    #[error("route proposal does not match the current design or its terminals")]
    InvalidRouteProposal,
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

fn check_saddle(design: &crate::Design, saddle: &Saddle) -> Result<(), OperationError> {
    let size = design.board.size;
    if saddle.start == saddle.end
        || [saddle.start, saddle.end]
            .iter()
            .any(|hole| hole.x >= size.holes_x() || hole.y >= size.holes_y())
    {
        return Err(OperationError::InvalidSaddle);
    }
    if let Some(reason) = crate::saddles::hole_conflict(design, saddle) {
        return Err(OperationError::SaddleHoleOccupied(reason));
    }
    if crate::saddles::crosses_pin(design, saddle) {
        return Err(OperationError::SaddleCrossesPin);
    }
    Ok(())
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
    let mut affected_constraints = BTreeSet::new();
    let summary = match &request.payload {
        OperationPayload::ResizeBoard {
            holes_x,
            holes_y,
            offset_x,
            offset_y,
        } => {
            let old_x = next.design.board.size.holes_x();
            let old_y = next.design.board.size.holes_y();
            let width_change = i64::from(*holes_x) - i64::from(old_x);
            let height_change = i64::from(*holes_y) - i64::from(old_y);
            if ![0, width_change].contains(offset_x) || ![0, height_change].contains(offset_y) {
                return Err(OperationError::InvalidBoardResizeOffset);
            }
            next.design.board.size =
                BoardSize::new(*holes_x, *holes_y).map_err(|_| OperationError::InvalidBoardSize)?;

            let translate_hole = |point: &mut HolePos| -> Result<(), OperationError> {
                let x = i64::from(point.x)
                    .checked_add(*offset_x)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(OperationError::BoardResizeOutOfRange)?;
                let y = i64::from(point.y)
                    .checked_add(*offset_y)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(OperationError::BoardResizeOutOfRange)?;
                *point = HolePos::new(x, y);
                Ok(())
            };
            let translate_route = |point: &mut RoutePos| -> Result<(), OperationError> {
                point.x2 = point
                    .x2
                    .checked_add(
                        offset_x
                            .checked_mul(2)
                            .ok_or(OperationError::BoardResizeOutOfRange)?,
                    )
                    .ok_or(OperationError::BoardResizeOutOfRange)?;
                point.y2 = point
                    .y2
                    .checked_add(
                        offset_y
                            .checked_mul(2)
                            .ok_or(OperationError::BoardResizeOutOfRange)?,
                    )
                    .ok_or(OperationError::BoardResizeOutOfRange)?;
                Ok(())
            };

            for instance in next.design.components.values_mut() {
                translate_hole(&mut instance.position)?;
                affected.insert(instance.id);
            }
            for junction in next.design.junctions.values_mut() {
                translate_hole(&mut junction.position)?;
                affected_nets.insert(junction.net_id);
            }
            for wire in next.design.wires.values_mut() {
                for vertex in &mut wire.vertices {
                    translate_route(vertex)?;
                }
                for terminal in [&mut wire.start, &mut wire.end] {
                    if let perfboard_core::TerminalRef::RailNode { position, .. } = terminal {
                        translate_route(position)?;
                    }
                }
                affected_nets.insert(wire.net_id);
                affected_wires.insert(wire.id);
            }
            for rail in next.design.rails.values_mut() {
                for vertex in &mut rail.vertices {
                    translate_route(vertex)?;
                }
                affected_nets.insert(rail.net_id);
            }
            for saddle in next.design.saddles.values_mut() {
                crate::saddles::translate(saddle, *offset_x, *offset_y)
                    .ok_or(OperationError::BoardResizeOutOfRange)?;
            }
            for hole in &mut next.design.board.features.mounting_holes {
                translate_hole(&mut hole.position)?;
            }
            for area in &mut next.design.board.features.forbidden_areas {
                translate_hole(&mut area.min)?;
                translate_hole(&mut area.max)?;
            }
            for constraint in next.design.constraints.values_mut() {
                match &mut constraint.kind {
                    ConstraintKind::FixedPlacement { position, .. } => {
                        translate_hole(position)?;
                    }
                    ConstraintKind::ReservedArea { area, .. }
                    | ConstraintKind::AssemblyAccess { area, .. } => {
                        translate_hole(&mut area.min)?;
                        translate_hole(&mut area.max)?;
                    }
                    _ => {}
                }
                affected_constraints.insert(constraint.id);
            }
            let choices = std::mem::take(&mut next.design.crossing_choices);
            for (_, mut choice) in choices {
                translate_route(&mut choice.position)?;
                let key = format!(
                    "{}:{}:{}:{}",
                    choice.first_wire_id,
                    choice.second_wire_id,
                    choice.position.x2,
                    choice.position.y2
                );
                next.design.crossing_choices.insert(key, choice);
            }
            format!("Resized board from {old_x} × {old_y} to {holes_x} × {holes_y} holes")
        }
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
            // A library definition that differs from the stored snapshot only
            // in metadata (name, designator, description, documentation,
            // version) replaces it, as "Update metadata" would; anything else
            // would change parts already placed.
            if let Some(existing) = next.design.component_definitions.get(&definition.id)
                && !existing.same_except_metadata(definition)
                && next
                    .design
                    .components
                    .values()
                    .any(|component| component.definition_id == definition.id)
            {
                return Err(OperationError::DefinitionConflict);
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
            let mut instance = instance.clone();
            if instance.name.trim().is_empty() {
                instance.name = definition.next_instance_name(
                    next.design
                        .components
                        .values()
                        .map(|component| component.name.as_str()),
                );
            }
            next.design
                .component_definitions
                .insert(definition.id.clone(), definition.clone());
            let summary = format!("Added {}", instance.name);
            affected.insert(instance.id);
            next.design.components.insert(instance.id, instance);
            summary
        }
        OperationPayload::UpdateComponentMetadata {
            instance_id,
            definition,
        } => {
            let instance = next
                .design
                .components
                .get(instance_id)
                .ok_or(OperationError::InstanceNotFound)?;
            if instance.definition_id != definition.id {
                return Err(OperationError::DefinitionMismatch);
            }
            let stored = next
                .design
                .component_definitions
                .get_mut(&instance.definition_id)
                .ok_or(OperationError::InstanceNotFound)?;
            stored
                .definition_version
                .clone_from(&definition.definition_version);
            stored.name.clone_from(&definition.name);
            stored.designator.clone_from(&definition.designator);
            stored.description.clone_from(&definition.description);
            stored.documentation.clone_from(&definition.documentation);
            // A plain two-pin part can become a shuntable jumper, or back, when
            // the library says so. Pins must match exactly, so nets and wires
            // stay valid; any other kind change still needs a new definition.
            let switches_jumper = matches!(
                (stored.kind, definition.kind),
                (ComponentKind::Standard, ComponentKind::Jumper)
                    | (ComponentKind::Jumper, ComponentKind::Standard)
            );
            if switches_jumper && stored.pins == definition.pins {
                stored.kind = definition.kind;
            }
            let is_jumper = stored.kind == ComponentKind::Jumper;
            stored
                .validate()
                .map_err(|error| DocumentError::InvalidComponentDefinition {
                    id: stored.id.clone(),
                    message: error.to_string(),
                })?;
            let definition_id = instance.definition_id.clone();
            let definition_name = stored.name.clone();
            if !is_jumper {
                for component in next.design.components.values_mut() {
                    if component.definition_id == definition_id {
                        component.shunted = false;
                    }
                }
            }
            affected.extend(
                next.design
                    .components
                    .values()
                    .filter(|component| component.definition_id == definition_id)
                    .map(|component| component.id),
            );
            format!("Updated {definition_name} component metadata")
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
        OperationPayload::SetJumperShunt {
            instance_id,
            shunted,
        } => {
            let instance = next
                .design
                .components
                .get_mut(instance_id)
                .ok_or(OperationError::InstanceNotFound)?;
            let is_jumper = next
                .design
                .component_definitions
                .get(&instance.definition_id)
                .is_some_and(|definition| definition.kind == ComponentKind::Jumper);
            if !is_jumper {
                return Err(OperationError::NotAJumper);
            }
            instance.shunted = *shunted;
            affected.insert(*instance_id);
            format!(
                "{} shunt on {}",
                if *shunted { "Fitted" } else { "Removed" },
                instance.name
            )
        }
        OperationPayload::DefineNet { net } => {
            if next.design.nets.contains_key(&net.id) {
                return Err(OperationError::DuplicateNet);
            }
            next.design.nets.insert(net.id, net.clone());
            let before = next.design.nets.clone();
            for pin in &net.pins {
                crate::connectivity::release_stale_bridge_pins(&mut next.design, net.id, pin);
            }
            affected_nets.insert(net.id);
            affected_nets.extend(
                next.design
                    .nets
                    .iter()
                    .filter(|(id, other)| before.get(id) != Some(*other))
                    .map(|(id, _)| *id),
            );
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
                    .rails
                    .values()
                    .any(|rail| rail.net_id == *net_id)
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
        OperationPayload::MergeNets {
            source_net_id,
            target_net_id,
        } => {
            if source_net_id == target_net_id {
                return Err(OperationError::MergeIntoSelf);
            }
            let design = &mut next.design;
            let target = design
                .nets
                .get(target_net_id)
                .ok_or(OperationError::NetNotFound)?;
            let source = design
                .nets
                .get(source_net_id)
                .ok_or(OperationError::NetNotFound)?;
            if source.state != ConnectionState::Accepted
                || target.state != ConnectionState::Accepted
            {
                return Err(OperationError::MergeProposedNet);
            }
            let source = design.nets.remove(source_net_id).expect("checked above");
            let target = design.nets.get_mut(target_net_id).expect("checked above");
            target.pins.extend(source.pins);
            let source_notes = source.notes.trim();
            if !source_notes.is_empty() && !target.notes.contains(source_notes) {
                if !target.notes.trim().is_empty() {
                    target.notes.push_str("\n\n");
                }
                target.notes.push_str(source_notes);
            }
            let target_name = target.name.clone();
            for wire in design.wires.values_mut() {
                if wire.net_id == *source_net_id {
                    wire.net_id = *target_net_id;
                    affected_wires.insert(wire.id);
                }
            }
            for rail in design.rails.values_mut() {
                if rail.net_id == *source_net_id {
                    rail.net_id = *target_net_id;
                }
            }
            for junction in design.junctions.values_mut() {
                if junction.net_id == *source_net_id {
                    junction.net_id = *target_net_id;
                }
            }
            for constraint in design.constraints.values_mut() {
                if let ConstraintKind::PreferredMaximumLength { net_id, .. }
                | ConstraintKind::WireSide { net_id, .. } = &mut constraint.kind
                    && *net_id == *source_net_id
                {
                    *net_id = *target_net_id;
                    affected_constraints.insert(constraint.id);
                }
            }
            affected_nets.insert(*source_net_id);
            affected_nets.insert(*target_net_id);
            format!("Merged net {} into {target_name}", source.name)
        }
        OperationPayload::RenameNet { net_id, name } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(OperationError::EmptyNetName);
            }
            if next
                .design
                .nets
                .values()
                .any(|net| net.id != *net_id && net.name.eq_ignore_ascii_case(name))
            {
                return Err(OperationError::NetNameTaken(name.to_owned()));
            }
            let net = next
                .design
                .nets
                .get_mut(net_id)
                .ok_or(OperationError::NetNotFound)?;
            let old = std::mem::replace(&mut net.name, name.to_owned());
            affected_nets.insert(*net_id);
            format!("Renamed net {old} to {name}")
        }
        OperationPayload::AssignPin { net_id, pin } => {
            let net = next
                .design
                .nets
                .get_mut(net_id)
                .ok_or(OperationError::NetNotFound)?;
            net.pins.insert(pin.clone());
            let message = format!("Assigned pin to net {}", net.name);
            let before = next.design.nets.clone();
            crate::connectivity::release_stale_bridge_pins(&mut next.design, *net_id, pin);
            affected_nets.insert(*net_id);
            affected_nets.extend(
                next.design
                    .nets
                    .iter()
                    .filter(|(id, net)| before.get(id) != Some(*net))
                    .map(|(id, _)| *id),
            );
            message
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
                || next
                    .design
                    .rails
                    .values()
                    .any(|rail| rail.net_id == *net_id)
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
            if !crate::connectivity::route_is_supported(&wire.vertices) {
                return Err(OperationError::UnsupportedWireRoute);
            }
            if crate::connectivity::route_crosses_unrelated_pin(&next.design, wire) {
                return Err(OperationError::WireCrossesPin);
            }
            next.design.wires.insert(wire.id, wire.clone());
            affected_nets.insert(wire.net_id);
            affected_wires.insert(wire.id);
            format!("Added wire {}", wire.id)
        }
        OperationPayload::AddSaddle { saddle } => {
            if next.design.saddles.contains_key(&saddle.id) {
                return Err(OperationError::DuplicateSaddle);
            }
            check_saddle(&next.design, saddle)?;
            next.design.saddles.insert(saddle.id, saddle.clone());
            format!("Added saddle {}", saddle.id)
        }
        OperationPayload::UpdateSaddle { saddle } => {
            if !next.design.saddles.contains_key(&saddle.id) {
                return Err(OperationError::SaddleNotFound);
            }
            check_saddle(&next.design, saddle)?;
            next.design.saddles.insert(saddle.id, saddle.clone());
            format!("Updated saddle {}", saddle.id)
        }
        OperationPayload::RemoveSaddle { saddle_id } => {
            next.design
                .saddles
                .remove(saddle_id)
                .ok_or(OperationError::SaddleNotFound)?;
            format!("Removed saddle {saddle_id}")
        }
        OperationPayload::RemoveWire { wire_id } => {
            if next
                .design
                .wires
                .get(wire_id)
                .is_some_and(|wire| wire.locked)
            {
                return Err(OperationError::LockedWire);
            }
            let wire = next
                .design
                .wires
                .remove(wire_id)
                .ok_or(OperationError::WireNotFound)?;
            next.design.crossing_choices.retain(|_, choice| {
                choice.first_wire_id != *wire_id && choice.second_wire_id != *wire_id
            });
            affected_nets.insert(wire.net_id);
            affected_wires.insert(*wire_id);
            format!("Removed wire {wire_id}")
        }
        OperationPayload::AddRail { rail } => {
            if next.design.rails.contains_key(&rail.id) {
                return Err(OperationError::DuplicateRail);
            }
            if !next.design.nets.contains_key(&rail.net_id) {
                return Err(OperationError::NetNotFound);
            }
            if !crate::connectivity::route_is_supported(&rail.vertices) {
                return Err(OperationError::UnsupportedRailRoute);
            }
            if crate::connectivity::rail_crosses_pin(&next.design, rail) {
                return Err(OperationError::RailCrossesPin);
            }
            next.design.rails.insert(rail.id, rail.clone());
            affected_nets.insert(rail.net_id);
            format!("Added rail {}", rail.id)
        }
        OperationPayload::RemoveRail { rail_id } => {
            let rail = next
                .design
                .rails
                .get(rail_id)
                .ok_or(OperationError::RailNotFound)?;
            if rail.locked {
                return Err(OperationError::LockedRail);
            }
            if next.design.wires.values().any(|wire| {
                [&wire.start, &wire.end].into_iter().any(|terminal| {
                    matches!(
                        terminal,
                        perfboard_core::TerminalRef::RailNode { rail_id: id, .. } if id == rail_id
                    )
                })
            }) {
                return Err(OperationError::ObjectInUse);
            }
            let rail = next.design.rails.remove(rail_id).expect("checked above");
            affected_nets.insert(rail.net_id);
            format!("Removed rail {rail_id}")
        }
        OperationPayload::ModifyRailPath { rail_id, vertices } => {
            let mut rail = next
                .design
                .rails
                .get(rail_id)
                .cloned()
                .ok_or(OperationError::RailNotFound)?;
            if rail.locked {
                return Err(OperationError::LockedRail);
            }
            if !crate::connectivity::route_is_supported(vertices) {
                return Err(OperationError::UnsupportedRailRoute);
            }
            rail.vertices.clone_from(vertices);
            if crate::connectivity::rail_crosses_pin(&next.design, &rail) {
                return Err(OperationError::RailCrossesPin);
            }
            let referenced: BTreeSet<RoutePos> = next
                .design
                .wires
                .values()
                .flat_map(|wire| [&wire.start, &wire.end])
                .filter_map(|terminal| match terminal {
                    perfboard_core::TerminalRef::RailNode {
                        rail_id: id,
                        position,
                    } if id == rail_id => Some(*position),
                    _ => None,
                })
                .collect();
            if referenced
                .iter()
                .any(|position| !vertices.contains(position))
            {
                return Err(OperationError::ObjectInUse);
            }
            next.design.rails.insert(*rail_id, rail.clone());
            affected_nets.insert(rail.net_id);
            format!("Edited rail {rail_id} path")
        }
        OperationPayload::SetRailColor { rail_id, color } => {
            let rail = next
                .design
                .rails
                .get_mut(rail_id)
                .ok_or(OperationError::RailNotFound)?;
            rail.color.clone_from(color);
            affected_nets.insert(rail.net_id);
            format!("Changed rail {rail_id} color")
        }
        OperationPayload::ModifyWirePath { wire_id, vertices } => {
            let mut candidate = next
                .design
                .wires
                .get(wire_id)
                .cloned()
                .ok_or(OperationError::WireNotFound)?;
            if candidate.locked {
                return Err(OperationError::LockedWire);
            }
            candidate.vertices.clone_from(vertices);
            if !crate::connectivity::route_is_supported(&candidate.vertices) {
                return Err(OperationError::UnsupportedWireRoute);
            }
            if crate::connectivity::route_crosses_unrelated_pin(&next.design, &candidate) {
                return Err(OperationError::WireCrossesPin);
            }
            next.design.wires.insert(*wire_id, candidate.clone());
            next.design.crossing_choices.retain(|_, choice| {
                choice.first_wire_id != *wire_id && choice.second_wire_id != *wire_id
            });
            affected_nets.insert(candidate.net_id);
            affected_wires.insert(*wire_id);
            format!("Edited wire {wire_id} path")
        }
        OperationPayload::ReconnectWireEndpoint {
            wire_id,
            endpoint,
            terminal,
        } => {
            let mut candidate = next
                .design
                .wires
                .get(wire_id)
                .cloned()
                .ok_or(OperationError::WireNotFound)?;
            if candidate.locked {
                return Err(OperationError::LockedWire);
            }

            let old_terminal = match endpoint {
                WireEndpoint::Start => candidate.start.clone(),
                WireEndpoint::End => candidate.end.clone(),
            };
            let other_terminal = match endpoint {
                WireEndpoint::Start => &candidate.end,
                WireEndpoint::End => &candidate.start,
            };
            if terminal == other_terminal {
                return Err(OperationError::InvalidWireTerminal);
            }

            if let perfboard_core::TerminalRef::Pin {
                component_id,
                pin_id,
            } = terminal
            {
                let pin = PinRef {
                    component_id: *component_id,
                    pin_id: pin_id.clone(),
                };
                if crate::connectivity::pin_position(&next.design, &pin).is_none() {
                    return Err(OperationError::InvalidWireTerminal);
                }
                if next
                    .design
                    .nets
                    .iter()
                    .any(|(net_id, net)| *net_id != candidate.net_id && net.pins.contains(&pin))
                {
                    return Err(OperationError::PinAssignedToDifferentNet);
                }
                next.design
                    .nets
                    .get_mut(&candidate.net_id)
                    .ok_or(OperationError::NetNotFound)?
                    .pins
                    .insert(pin);
            }

            let target_position =
                crate::connectivity::terminal_position(&next.design, candidate.net_id, terminal)
                    .map_err(|_| OperationError::InvalidWireTerminal)?;
            match endpoint {
                WireEndpoint::Start => {
                    candidate.start.clone_from(terminal);
                    candidate.vertices[0] = target_position;
                }
                WireEndpoint::End => {
                    candidate.end.clone_from(terminal);
                    *candidate
                        .vertices
                        .last_mut()
                        .expect("validated wire has vertices") = target_position;
                }
            }
            candidate.vertices.dedup();
            if !crate::connectivity::route_is_supported(&candidate.vertices) {
                return Err(OperationError::UnsupportedWireRoute);
            }
            if crate::connectivity::route_crosses_unrelated_pin(&next.design, &candidate) {
                return Err(OperationError::WireCrossesPin);
            }
            next.design.wires.insert(*wire_id, candidate.clone());

            if let perfboard_core::TerminalRef::Pin {
                component_id,
                pin_id,
            } = old_terminal
            {
                let old_pin = PinRef {
                    component_id,
                    pin_id,
                };
                let old_terminal = perfboard_core::TerminalRef::from(old_pin.clone());
                let still_connected = next.design.wires.values().any(|wire| {
                    wire.net_id == candidate.net_id
                        && (wire.start == old_terminal || wire.end == old_terminal)
                });
                if !still_connected && let Some(net) = next.design.nets.get_mut(&candidate.net_id) {
                    net.pins.remove(&old_pin);
                }
            }
            next.design.crossing_choices.retain(|_, choice| {
                choice.first_wire_id != *wire_id && choice.second_wire_id != *wire_id
            });
            affected_nets.insert(candidate.net_id);
            affected_wires.insert(*wire_id);
            format!("Reconnected {endpoint:?} endpoint of wire {wire_id}")
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
            if wire.locked {
                return Err(OperationError::LockedWire);
            }
            wire.side = *side;
            next.design.crossing_choices.retain(|_, choice| {
                choice.first_wire_id != *wire_id && choice.second_wire_id != *wire_id
            });
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
        OperationPayload::AddConstraint { constraint } => {
            if next
                .design
                .constraints
                .insert(constraint.id, constraint.clone())
                .is_some()
            {
                return Err(OperationError::DuplicateConstraint);
            }
            affected_constraints.insert(constraint.id);
            format!("Added constraint {}", constraint.id)
        }
        OperationPayload::UpdateConstraint { constraint } => {
            if !next.design.constraints.contains_key(&constraint.id) {
                return Err(OperationError::ConstraintNotFound);
            }
            next.design
                .constraints
                .insert(constraint.id, constraint.clone());
            affected_constraints.insert(constraint.id);
            format!("Updated constraint {}", constraint.id)
        }
        OperationPayload::RemoveConstraint { constraint_id } => {
            next.design
                .constraints
                .remove(constraint_id)
                .ok_or(OperationError::ConstraintNotFound)?;
            affected_constraints.insert(*constraint_id);
            format!("Removed constraint {constraint_id}")
        }
        OperationPayload::UpsertJumperInventory { entry } => {
            next.design
                .jumper_inventory
                .insert(entry.id.clone(), entry.clone());
            format!("Updated jumper inventory {}", entry.id)
        }
        OperationPayload::RemoveJumperInventory { inventory_id } => {
            next.design
                .jumper_inventory
                .remove(inventory_id)
                .ok_or(OperationError::JumperInventoryNotFound)?;
            format!("Removed jumper inventory {inventory_id}")
        }
        OperationPayload::AcceptRouteProposal {
            proposal,
            wire_id,
            color,
            notes,
            replace_wire_id,
            new_route_over,
        } => {
            if !crate::connectivity::route_is_supported(&proposal.vertices) {
                return Err(OperationError::UnsupportedWireRoute);
            }
            let request = crate::RouteRequest {
                net_id: proposal.net_id,
                start: proposal.start.clone(),
                end: proposal.end.clone(),
                preferred_side: Some(proposal.side),
                replace_wire_id: *replace_wire_id,
            };
            let evaluated = crate::evaluate_route(
                &next.design,
                &request,
                proposal.side,
                proposal.vertices.clone(),
                &crate::RoutingCostConfig::default(),
            );
            if evaluated != *proposal {
                return Err(OperationError::InvalidRouteProposal);
            }
            if next.design.wires.contains_key(wire_id) && Some(*wire_id) != *replace_wire_id {
                return Err(OperationError::DuplicateWire);
            }
            let routed_wire = Wire {
                id: *wire_id,
                net_id: proposal.net_id,
                start: proposal.start.clone(),
                end: proposal.end.clone(),
                side: proposal.side,
                color: color.clone(),
                vertices: proposal.vertices.clone(),
                locked: false,
                notes: notes.clone(),
            };
            if crate::connectivity::route_crosses_unrelated_pin(&next.design, &routed_wire) {
                return Err(OperationError::WireCrossesPin);
            }
            if let Some(replaced) = replace_wire_id {
                let removed = next
                    .design
                    .wires
                    .remove(replaced)
                    .ok_or(OperationError::WireNotFound)?;
                next.design.crossing_choices.retain(|_, choice| {
                    choice.first_wire_id != *replaced && choice.second_wire_id != *replaced
                });
                affected_wires.insert(removed.id);
            }
            next.design.wires.insert(*wire_id, routed_wire);
            if let Some(route_is_over) = new_route_over {
                for crossing in &proposal.crossings {
                    let (first, second) = if wire_id < &crossing.other_wire_id {
                        (*wire_id, crossing.other_wire_id)
                    } else {
                        (crossing.other_wire_id, *wire_id)
                    };
                    let key = format!(
                        "{first}:{second}:{}:{}",
                        crossing.position.x2, crossing.position.y2
                    );
                    next.design.crossing_choices.insert(
                        key,
                        perfboard_core::CrossingChoice {
                            first_wire_id: first,
                            second_wire_id: second,
                            over_wire_id: if *route_is_over {
                                *wire_id
                            } else {
                                crossing.other_wire_id
                            },
                            position: crossing.position,
                        },
                    );
                }
            }
            affected_nets.insert(proposal.net_id);
            affected_wires.insert(*wire_id);
            format!("Accepted route proposal as wire {wire_id}")
        }
    };
    next.validation = next.calculate_validation();
    if (!request.allow_invalid_placement
        && next
            .validation
            .iter()
            .any(|item| item.severity == DiagnosticSeverity::Error))
        || next
            .validation
            .iter()
            .any(|item| item.code.0 == "constraint.required")
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
        affected_constraint_ids: affected_constraints,
        validation: document.validation.clone(),
        change_summary: summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Board;
    use perfboard_core::{BoardSize, MountingHole, ProjectId};
    use uuid::Uuid;

    fn point_definition(name: &str) -> ComponentDefinition {
        ComponentDefinition::from_toml(&format!(
            "schema_version=1\nid='test.point'\ndefinition_version='1'\nname='{name}'\nallowed_rotations=['deg0']\n[body]\nx=-0.5\ny=-0.5\nwidth=1\nheight=1\n[[pins]]\nid='p'\nnumber='1'\nname='P'\nx=0\ny=0"
        ))
        .unwrap()
    }

    fn point_instance(definition: &ComponentDefinition, x: u32) -> ComponentInstance {
        ComponentInstance {
            id: ComponentInstanceId::new(),
            definition_id: definition.id.clone(),
            name: definition.name.clone(),
            position: HolePos::new(x, 1),
            rotation: Rotation::Deg0,
            properties: BTreeMap::new(),
            notes: String::new(),
            shunted: false,
        }
    }

    fn operation(document: &ProjectDocument, payload: OperationPayload) -> OperationRequest {
        OperationRequest {
            operation_id: OperationId::new(),
            expected_revision: document.revision,
            origin: OperationOrigin::User,
            rationale: None,
            allow_invalid_placement: false,
            payload,
        }
    }

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

    #[test]
    fn adding_changed_definition_replaces_unused_snapshot() {
        let mut document = ProjectDocument::new(
            ProjectId::from_uuid(Uuid::nil()),
            "x",
            Board::new("x", BoardSize::new(4, 4).unwrap(), 2.54),
        );
        let old_definition = point_definition("Old point");
        let old_instance = point_instance(&old_definition, 1);
        let old_instance_id = old_instance.id;

        let request = operation(
            &document,
            OperationPayload::AddComponent {
                definition: old_definition,
                instance: old_instance,
            },
        );
        apply_operation(&mut document, request).unwrap();
        let request = operation(
            &document,
            OperationPayload::RemoveComponent {
                instance_id: old_instance_id,
            },
        );
        apply_operation(&mut document, request).unwrap();

        let changed_definition = point_definition("Changed point");
        let changed_instance = point_instance(&changed_definition, 2);
        let request = operation(
            &document,
            OperationPayload::AddComponent {
                definition: changed_definition.clone(),
                instance: changed_instance,
            },
        );
        apply_operation(&mut document, request).unwrap();

        assert_eq!(
            document.design.component_definitions[&changed_definition.id],
            changed_definition
        );
    }

    #[test]
    fn adding_changed_definition_conflicts_with_placed_snapshot() {
        let mut document = ProjectDocument::new(
            ProjectId::from_uuid(Uuid::nil()),
            "x",
            Board::new("x", BoardSize::new(4, 4).unwrap(), 2.54),
        );
        let old_definition = point_definition("Old point");
        let old_instance = point_instance(&old_definition, 1);
        let request = operation(
            &document,
            OperationPayload::AddComponent {
                definition: old_definition,
                instance: old_instance,
            },
        );
        apply_operation(&mut document, request).unwrap();

        let mut changed_definition = point_definition("Old point");
        changed_definition.body.width = 2.0;
        let changed_instance = point_instance(&changed_definition, 2);
        let request = operation(
            &document,
            OperationPayload::AddComponent {
                definition: changed_definition,
                instance: changed_instance,
            },
        );
        assert!(matches!(
            apply_operation(&mut document, request),
            Err(OperationError::DefinitionConflict)
        ));
    }

    fn empty_document() -> ProjectDocument {
        ProjectDocument::new(
            ProjectId::from_uuid(Uuid::nil()),
            "x",
            Board::new("x", BoardSize::new(4, 4).unwrap(), 2.54),
        )
    }

    fn add_unnamed(document: &mut ProjectDocument, definition: &ComponentDefinition, x: u32) {
        let mut instance = point_instance(definition, x);
        instance.name = " ".into();
        let request = operation(
            document,
            OperationPayload::AddComponent {
                definition: definition.clone(),
                instance,
            },
        );
        apply_operation(document, request).unwrap();
    }

    fn names(document: &ProjectDocument) -> Vec<String> {
        let mut names = document
            .design
            .components
            .values()
            .map(|component| component.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn unnamed_parts_take_the_next_free_designator() {
        let mut document = empty_document();
        let mut definition = point_definition("Point");
        definition.designator = "TP".into();
        add_unnamed(&mut document, &definition, 1);
        add_unnamed(&mut document, &definition, 2);
        let first = document
            .design
            .components
            .values()
            .find(|component| component.name == "TP1")
            .unwrap()
            .id;
        let request = operation(
            &document,
            OperationPayload::RemoveComponent { instance_id: first },
        );
        apply_operation(&mut document, request).unwrap();
        add_unnamed(&mut document, &definition, 3);
        assert_eq!(names(&document), ["TP1", "TP2"]);
    }

    #[test]
    fn unnamed_parts_without_a_designator_are_named_after_the_definition() {
        let mut document = empty_document();
        let definition = point_definition("Point");
        add_unnamed(&mut document, &definition, 1);
        add_unnamed(&mut document, &definition, 2);
        assert_eq!(names(&document), ["Point 1", "Point 2"]);
    }

    #[test]
    fn a_metadata_only_library_change_refreshes_the_snapshot_when_placing() {
        let mut document = empty_document();
        let old = point_definition("Point");
        add_unnamed(&mut document, &old, 1);

        let mut newer = point_definition("Test point");
        newer.designator = "TP".into();
        newer.definition_version = "2".into();
        add_unnamed(&mut document, &newer, 2);

        assert_eq!(document.design.component_definitions[&newer.id], newer);
        assert_eq!(names(&document), ["Point 1", "TP1"]);
    }

    #[test]
    fn updating_component_metadata_preserves_pins_geometry_and_instances() {
        let mut document = ProjectDocument::new(
            ProjectId::from_uuid(Uuid::nil()),
            "x",
            Board::new("x", BoardSize::new(6, 4).unwrap(), 2.54),
        );
        let old_definition = point_definition("Old point");
        let first = point_instance(&old_definition, 1);
        let second = point_instance(&old_definition, 4);
        for instance in [first.clone(), second.clone()] {
            let request = operation(
                &document,
                OperationPayload::AddComponent {
                    definition: old_definition.clone(),
                    instance,
                },
            );
            apply_operation(&mut document, request).unwrap();
        }

        let mut library_definition = old_definition.clone();
        library_definition.definition_version = "2".into();
        library_definition.name = "New point".into();
        library_definition.description = "Updated description".into();
        library_definition.documentation = "Updated documentation".into();
        library_definition.body.x = 42.0;
        library_definition.pins[0].name = "Changed pin".into();
        library_definition.pins[0].x = 9;

        let request = operation(
            &document,
            OperationPayload::UpdateComponentMetadata {
                instance_id: first.id,
                definition: library_definition.clone(),
            },
        );
        let result = apply_operation(&mut document, request).unwrap();

        let mut expected = old_definition;
        expected.definition_version = library_definition.definition_version;
        expected.name = library_definition.name;
        expected.description = library_definition.description;
        expected.documentation = library_definition.documentation;
        assert_eq!(
            document.design.component_definitions[&expected.id],
            expected
        );
        assert_eq!(document.design.components[&first.id], first);
        assert_eq!(document.design.components[&second.id], second);
        assert_eq!(
            result.affected_component_ids,
            BTreeSet::from([first.id, second.id])
        );
    }

    #[test]
    fn resizing_from_top_left_translates_board_content_and_is_undoable() {
        let mut document = ProjectDocument::new(
            ProjectId::from_uuid(Uuid::nil()),
            "x",
            Board::new("x", BoardSize::new(3, 2).unwrap(), 2.54),
        );
        document
            .design
            .board
            .features
            .mounting_holes
            .push(MountingHole {
                position: HolePos::new(0, 0),
                diameter_mm: 1.0,
                label: "corner".into(),
            });
        let result = apply_operation(
            &mut document,
            OperationRequest {
                operation_id: OperationId::new(),
                expected_revision: DocumentRevision(0),
                origin: OperationOrigin::User,
                rationale: None,
                allow_invalid_placement: false,
                payload: OperationPayload::ResizeBoard {
                    holes_x: 5,
                    holes_y: 3,
                    offset_x: 2,
                    offset_y: 1,
                },
            },
        )
        .expect("valid resize");

        assert_eq!(document.design.board.size.holes_x(), 5);
        assert_eq!(document.design.board.size.holes_y(), 3);
        assert_eq!(
            document.design.board.features.mounting_holes[0].position,
            HolePos::new(2, 1)
        );
        assert_eq!(
            result.change_summary,
            "Resized board from 3 × 2 to 5 × 3 holes"
        );
        assert_eq!(document.undo_stack.len(), 1);
    }
}
