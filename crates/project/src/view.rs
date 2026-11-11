//! Lightweight read model of a project for interactive clients.
//!
//! A saved [`ProjectDocument`] embeds a full design snapshot in every undo and
//! redo entry and in every commit. Shipping all of that to the editor after
//! each edit is wasteful once a project has some history, so the editor reads
//! this view instead: the current design plus compact summaries of history.

use perfboard_core::{CommitId, Diagnostic, ProjectId};
use serde::Serialize;

use crate::{
    Design, DocumentRevision, OperationOrigin, ProjectDocument, SchemaVersion, SemanticDiff,
    WorkingChange,
};

/// One undo or redo entry without its design snapshots.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChangeSummary {
    pub operation_id: String,
    pub origin: OperationOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub summary: String,
    /// Objects touched by this entry, derived from its before/after snapshots.
    pub diff: SemanticDiff,
}

/// A commit without its design snapshot and recorded changes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CommitSummary {
    pub id: CommitId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<CommitId>,
    pub name: String,
    pub description: String,
    pub timestamp_ms: u64,
    pub source_revision: DocumentRevision,
    pub change_count: usize,
    /// Distinct origins of the recorded changes, in first-seen order.
    pub origins: Vec<OperationOrigin>,
    /// Rationales supplied with the recorded changes (MCP explanations).
    pub rationales: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProjectView {
    pub schema_version: SchemaVersion,
    pub project_id: ProjectId,
    pub name: String,
    pub revision: DocumentRevision,
    pub design: Design,
    pub validation: Vec<Diagnostic>,
    /// Commits in chronological order.
    pub commits: Vec<CommitSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_commit: Option<CommitId>,
    /// Design of the head commit, for the committed pane and comparisons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_design: Option<Design>,
    /// Undo entries, oldest first (the last entry is undone next).
    pub undo: Vec<ChangeSummary>,
    /// Redo entries, oldest first (the last entry is redone next).
    pub redo: Vec<ChangeSummary>,
}

fn summarize(change: &WorkingChange) -> ChangeSummary {
    ChangeSummary {
        operation_id: change.operation_id.to_string(),
        origin: change.origin,
        rationale: change.rationale.clone(),
        summary: change.summary.clone(),
        diff: SemanticDiff::between(&change.before, &change.after),
    }
}

impl ProjectView {
    #[must_use]
    pub fn from_document(document: &ProjectDocument) -> Self {
        let commits = document
            .commit_order
            .iter()
            .filter_map(|id| document.commits.get(id))
            .map(|commit| {
                let mut origins = Vec::new();
                let mut rationales = Vec::new();
                for change in &commit.changes {
                    if !origins.contains(&change.origin) {
                        origins.push(change.origin);
                    }
                    if let Some(rationale) = change.rationale.as_ref().filter(|r| !r.is_empty()) {
                        rationales.push(rationale.clone());
                    }
                }
                CommitSummary {
                    id: commit.id,
                    parent: commit.parent,
                    name: commit.name.clone(),
                    description: commit.description.clone(),
                    timestamp_ms: commit.timestamp_ms,
                    source_revision: commit.source_revision,
                    change_count: commit.changes.len(),
                    origins,
                    rationales,
                }
            })
            .collect();
        Self {
            schema_version: document.schema_version,
            project_id: document.project_id,
            name: document.name.clone(),
            revision: document.revision,
            design: document.design.clone(),
            validation: document.validation.clone(),
            commits,
            head_commit: document.head_commit,
            head_design: document
                .head_commit
                .and_then(|id| document.commits.get(&id))
                .map(|commit| commit.design.clone()),
            undo: document.undo_stack.iter().map(summarize).collect(),
            redo: document.redo_stack.iter().map(summarize).collect(),
        }
    }
}

/// Approximate serialized size of the undo and redo stacks in bytes.
#[must_use]
pub fn undo_history_bytes(document: &ProjectDocument) -> usize {
    let count = |stack: &[WorkingChange]| {
        stack
            .iter()
            .map(|change| serde_json::to_vec(change).map_or(0, |bytes| bytes.len()))
            .sum::<usize>()
    };
    count(&document.undo_stack) + count(&document.redo_stack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Board, OperationRequest, ProjectSession};
    use perfboard_core::BoardSize;

    fn document() -> ProjectDocument {
        ProjectDocument::new(
            ProjectId::new(),
            "View test",
            Board::new("10x8", BoardSize::new(10, 8).unwrap(), 2.54),
        )
    }

    #[test]
    fn view_omits_snapshots_but_summarizes_history() {
        let mut session = ProjectSession::new(document());
        let request: OperationRequest = serde_json::from_value(serde_json::json!({
            "operation_id": uuid::Uuid::new_v4(),
            "expected_revision": 0,
            "origin": "mcp",
            "rationale": "Make room for a connector",
            "type": "resize_board",
            "holes_x": 12,
            "holes_y": 8,
            "offset_x": 0,
            "offset_y": 0
        }))
        .unwrap();
        session.apply(request).unwrap();

        let view = ProjectView::from_document(session.document());
        assert_eq!(view.undo.len(), 1);
        assert_eq!(view.undo[0].origin, OperationOrigin::Mcp);
        assert_eq!(
            view.undo[0].rationale.as_deref(),
            Some("Make room for a connector")
        );
        assert!(view.head_design.is_none());

        let json = serde_json::to_value(&view).unwrap();
        assert!(json["undo"][0].get("before").is_none());
        assert!(undo_history_bytes(session.document()) > 0);
    }
}
