//! Serialized form of [`ProjectDocument`] (ADR 0013).
//!
//! In memory every undo/redo entry holds two full design snapshots and their
//! validation reports. Written out as-is, a project with a few dozen edits
//! repeats its design hundreds of times. The serialized form therefore stores
//! each snapshot as an RFC 6902 JSON patch from its neighbour, chained from the
//! current design, and drops the validation reports, which are deterministic
//! and recomputed on load. The working base is a patch from the current design
//! too. Commits keep their full design.
//!
//! Chain order, starting from the current `design`:
//!
//! - `undo_stack` is walked newest (last) to oldest. For each entry,
//!   `after_patch` turns the running design into the entry's `after`, then
//!   `before_patch` turns that into its `before`, which becomes the running
//!   design for the next (older) entry.
//! - `redo_stack` is walked from the next entry to redo (last) to the first.
//!   `before_patch` turns the running design into `before`, then
//!   `after_patch` turns that into `after`, which becomes the running design.
//!
//! In a contiguous history the first patch of every entry is empty and is
//! omitted, as is any empty patch.
//!
//! Schema version 1 files, which stored the snapshots in full, are still read
//! and upgraded in memory; they are written back as the current version.

use std::collections::BTreeMap;

use json_patch::PatchOperation;
use perfboard_core::{CommitId, Diagnostic, OperationId, ProjectId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{
    Commit, Design, DocumentRevision, OperationOrigin, OperationPayload, PROJECT_SCHEMA_VERSION,
    ProjectDocument, SchemaVersion, WorkingChange,
    document::{OLDEST_READABLE_SCHEMA_VERSION, calculate_design_validation},
};

/// One undo/redo entry as serialized.
#[derive(Serialize, Deserialize)]
struct StoredChange {
    operation_id: OperationId,
    origin: OperationOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rationale: Option<String>,
    summary: String,
    payloads: Vec<OperationPayload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    before_patch: Vec<PatchOperation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    after_patch: Vec<PatchOperation>,
    /// Schema version 1 only: the full snapshots. Their validation reports
    /// are ignored and recomputed.
    #[serde(default, skip_serializing)]
    before: Option<Design>,
    #[serde(default, skip_serializing)]
    after: Option<Design>,
}

/// Borrowing view used for writing, so saving does not clone the design or
/// the commits.
#[derive(Serialize)]
struct StoredDocumentOut<'a> {
    schema_version: SchemaVersion,
    project_id: ProjectId,
    name: &'a str,
    revision: DocumentRevision,
    design: &'a Design,
    #[serde(skip_serializing_if = "<[Diagnostic]>::is_empty")]
    validation: &'a [Diagnostic],
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    commits: &'a BTreeMap<CommitId, Commit>,
    #[serde(skip_serializing_if = "<[CommitId]>::is_empty")]
    commit_order: &'a [CommitId],
    #[serde(skip_serializing_if = "Option::is_none")]
    head_commit: Option<CommitId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_base_patch: Option<Vec<PatchOperation>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    undo_stack: Vec<StoredChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    redo_stack: Vec<StoredChange>,
}

#[derive(Deserialize)]
struct StoredDocumentIn {
    schema_version: SchemaVersion,
    project_id: ProjectId,
    name: String,
    revision: DocumentRevision,
    design: Design,
    #[serde(default)]
    validation: Vec<Diagnostic>,
    #[serde(default)]
    commits: BTreeMap<CommitId, Commit>,
    #[serde(default)]
    commit_order: Vec<CommitId>,
    #[serde(default)]
    head_commit: Option<CommitId>,
    #[serde(default)]
    working_base_patch: Option<Vec<PatchOperation>>,
    /// Schema version 1 only.
    #[serde(default)]
    working_base: Option<Design>,
    #[serde(default)]
    undo_stack: Vec<StoredChange>,
    #[serde(default)]
    redo_stack: Vec<StoredChange>,
}

#[derive(Clone, Copy)]
enum Stack {
    Undo,
    Redo,
}

impl Serialize for ProjectDocument {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error as _;
        let design = serde_json::to_value(&self.design).map_err(S::Error::custom)?;
        let working_base_patch = self
            .working_base
            .as_ref()
            .map(|base| serde_json::to_value(base).map(|base| diff(&design, &base)))
            .transpose()
            .map_err(S::Error::custom)?;
        StoredDocumentOut {
            schema_version: self.schema_version,
            project_id: self.project_id,
            name: &self.name,
            revision: self.revision,
            design: &self.design,
            validation: &self.validation,
            commits: &self.commits,
            commit_order: &self.commit_order,
            head_commit: self.head_commit,
            working_base_patch,
            undo_stack: encode_stack(&design, &self.undo_stack, Stack::Undo)
                .map_err(S::Error::custom)?,
            redo_stack: encode_stack(&design, &self.redo_stack, Stack::Redo)
                .map_err(S::Error::custom)?,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProjectDocument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        StoredDocumentIn::deserialize(deserializer)?
            .decode()
            .map_err(D::Error::custom)
    }
}

/// Serialized size of the undo and redo stacks, as they are written to the
/// project file.
pub(crate) fn encoded_history_bytes(document: &ProjectDocument) -> usize {
    let Ok(design) = serde_json::to_value(&document.design) else {
        return 0;
    };
    [
        (&document.undo_stack, Stack::Undo),
        (&document.redo_stack, Stack::Redo),
    ]
    .into_iter()
    .filter_map(|(stack, kind)| encode_stack(&design, stack, kind).ok())
    .filter_map(|stack| serde_json::to_vec(&stack).ok())
    .map(|bytes| bytes.len())
    .sum()
}

fn diff(from: &Value, to: &Value) -> Vec<PatchOperation> {
    json_patch::diff(from, to).0
}

fn encode_stack(
    design: &Value,
    stack: &[WorkingChange],
    kind: Stack,
) -> Result<Vec<StoredChange>, serde_json::Error> {
    let mut running = design.clone();
    let mut encoded = Vec::with_capacity(stack.len());
    for change in stack.iter().rev() {
        let (first, second) = match kind {
            Stack::Undo => (&change.after, &change.before),
            Stack::Redo => (&change.before, &change.after),
        };
        let first = serde_json::to_value(first)?;
        let second = serde_json::to_value(second)?;
        let first_patch = diff(&running, &first);
        let second_patch = diff(&first, &second);
        let (before_patch, after_patch) = match kind {
            Stack::Undo => (second_patch, first_patch),
            Stack::Redo => (first_patch, second_patch),
        };
        encoded.push(StoredChange {
            operation_id: change.operation_id,
            origin: change.origin,
            rationale: change.rationale.clone(),
            summary: change.summary.clone(),
            payloads: change.payloads.clone(),
            before_patch,
            after_patch,
            before: None,
            after: None,
        });
        running = second;
    }
    encoded.reverse();
    Ok(encoded)
}

/// Rebuilds full snapshots from a patch chain. Validation is recomputed only
/// where a patch actually changed the design.
fn decode_stack(
    design: &Design,
    stack: Vec<StoredChange>,
    kind: Stack,
) -> Result<Vec<WorkingChange>, String> {
    let name = match kind {
        Stack::Undo => "undo",
        Stack::Redo => "redo",
    };
    let mut running = serde_json::to_value(design).map_err(|error| error.to_string())?;
    let mut running_design = design.clone();
    let mut running_validation = calculate_design_validation(design);
    let mut step = |patch: &[PatchOperation]| -> Result<(Design, Vec<Diagnostic>), String> {
        if !patch.is_empty() {
            json_patch::patch(&mut running, patch)
                .map_err(|error| format!("invalid {name} history patch: {error}"))?;
            running_design = Design::deserialize(&running)
                .map_err(|error| format!("invalid {name} history snapshot: {error}"))?;
            running_validation = calculate_design_validation(&running_design);
        }
        Ok((running_design.clone(), running_validation.clone()))
    };
    let mut decoded = Vec::with_capacity(stack.len());
    for change in stack.into_iter().rev() {
        if change.before.is_some() || change.after.is_some() {
            return Err(format!(
                "{name} history mixes full snapshots into a version {} document",
                PROJECT_SCHEMA_VERSION.0
            ));
        }
        let ((before, before_validation), (after, after_validation)) = match kind {
            Stack::Undo => {
                let after = step(&change.after_patch)?;
                (step(&change.before_patch)?, after)
            }
            Stack::Redo => {
                let before = step(&change.before_patch)?;
                (before, step(&change.after_patch)?)
            }
        };
        decoded.push(WorkingChange {
            operation_id: change.operation_id,
            origin: change.origin,
            rationale: change.rationale,
            summary: change.summary,
            payloads: change.payloads,
            before,
            after,
            before_validation,
            after_validation,
        });
    }
    decoded.reverse();
    Ok(decoded)
}

/// Schema version 1 stored full snapshots; only their validation reports
/// need recomputing.
fn decode_v1_stack(stack: Vec<StoredChange>) -> Result<Vec<WorkingChange>, String> {
    stack
        .into_iter()
        .map(|change| {
            let (Some(before), Some(after)) = (change.before, change.after) else {
                return Err("version 1 history entry lacks its before/after snapshots".into());
            };
            Ok(WorkingChange {
                operation_id: change.operation_id,
                origin: change.origin,
                rationale: change.rationale,
                summary: change.summary,
                payloads: change.payloads,
                before_validation: calculate_design_validation(&before),
                after_validation: calculate_design_validation(&after),
                before,
                after,
            })
        })
        .collect()
}

impl StoredDocumentIn {
    fn decode(self) -> Result<ProjectDocument, String> {
        let mut document = ProjectDocument {
            schema_version: self.schema_version,
            project_id: self.project_id,
            name: self.name,
            revision: self.revision,
            design: self.design,
            validation: self.validation,
            commits: self.commits,
            commit_order: self.commit_order,
            head_commit: self.head_commit,
            working_base: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        };
        if self.schema_version == PROJECT_SCHEMA_VERSION {
            if self.working_base.is_some() {
                return Err(
                    "`working_base` is a version 1 field; expected `working_base_patch`".into(),
                );
            }
            if let Some(patch) = self.working_base_patch {
                let mut base =
                    serde_json::to_value(&document.design).map_err(|error| error.to_string())?;
                json_patch::patch(&mut base, &patch)
                    .map_err(|error| format!("invalid working base patch: {error}"))?;
                document.working_base = Some(
                    Design::deserialize(&base)
                        .map_err(|error| format!("invalid working base: {error}"))?,
                );
            }
            document.undo_stack = decode_stack(&document.design, self.undo_stack, Stack::Undo)?;
            document.redo_stack = decode_stack(&document.design, self.redo_stack, Stack::Redo)?;
        } else if self.schema_version == OLDEST_READABLE_SCHEMA_VERSION {
            document.working_base = self.working_base;
            document.undo_stack = decode_v1_stack(self.undo_stack)?;
            document.redo_stack = decode_v1_stack(self.redo_stack)?;
            document.schema_version = PROJECT_SCHEMA_VERSION;
        }
        // Any other version is left without history so that
        // `ProjectDocument::validate` rejects it with `UnsupportedSchema`.
        Ok(document)
    }
}
