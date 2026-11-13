use perfboard_core::{CommitId, Diagnostic, OperationId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Design, DocumentRevision, OperationOrigin, OperationPayload, ProjectDocument};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Commit {
    pub id: CommitId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<CommitId>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub timestamp_ms: u64,
    pub source_revision: DocumentRevision,
    pub design: Design,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<WorkingChange>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkingChange {
    pub operation_id: OperationId,
    pub origin: OperationOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub summary: String,
    pub payloads: Vec<OperationPayload>,
    pub before: Design,
    pub after: Design,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before_validation: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_validation: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryActionResult {
    pub revision: DocumentRevision,
    pub summary: String,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RevisionError {
    #[error("expected revision {expected:?}, but current revision is {actual:?}")]
    StaleRevision {
        expected: DocumentRevision,
        actual: DocumentRevision,
    },
    #[error("there is no working-copy change to undo")]
    NothingToUndo,
    #[error("there is no working-copy change to redo")]
    NothingToRedo,
    #[error("commit name must not be blank")]
    BlankCommitName,
    #[error("commit was not found")]
    CommitNotFound,
    #[error("project name must not be blank")]
    BlankProjectName,
}

#[allow(clippy::missing_errors_doc)] // Methods return the self-explanatory RevisionError variants.
impl ProjectDocument {
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.working_base
            .as_ref()
            .is_some_and(|base| base != &self.design)
            || (!self.undo_stack.is_empty() && self.working_base.is_none())
    }

    fn check_revision(&self, expected: DocumentRevision) -> Result<(), RevisionError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(RevisionError::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }

    fn history_result(&self, summary: String) -> HistoryActionResult {
        HistoryActionResult {
            revision: self.revision,
            summary,
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            dirty: self.is_dirty(),
        }
    }

    pub fn undo(
        &mut self,
        expected_revision: DocumentRevision,
    ) -> Result<HistoryActionResult, RevisionError> {
        self.check_revision(expected_revision)?;
        let change = self.undo_stack.pop().ok_or(RevisionError::NothingToUndo)?;
        self.design.clone_from(&change.before);
        self.validation.clone_from(&change.before_validation);
        let summary = format!("Undid {}", change.summary);
        self.redo_stack.push(change);
        self.revision.0 += 1;
        Ok(self.history_result(summary))
    }

    pub fn redo(
        &mut self,
        expected_revision: DocumentRevision,
    ) -> Result<HistoryActionResult, RevisionError> {
        self.check_revision(expected_revision)?;
        let change = self.redo_stack.pop().ok_or(RevisionError::NothingToRedo)?;
        self.design.clone_from(&change.after);
        self.validation.clone_from(&change.after_validation);
        let summary = format!("Redid {}", change.summary);
        self.undo_stack.push(change);
        self.revision.0 += 1;
        Ok(self.history_result(summary))
    }

    pub fn trim_undo_history(
        &mut self,
        expected_revision: DocumentRevision,
        keep_entries: usize,
    ) -> Result<HistoryActionResult, RevisionError> {
        self.check_revision(expected_revision)?;
        let undo_removed = self.undo_stack.len().saturating_sub(keep_entries);
        let redo_removed = self.redo_stack.len().saturating_sub(keep_entries);
        if undo_removed > 0 {
            self.undo_stack.drain(..undo_removed);
        }
        if redo_removed > 0 {
            self.redo_stack.drain(..redo_removed);
        }
        let removed = undo_removed + redo_removed;
        if removed > 0 {
            self.revision.0 += 1;
        }
        let summary = if keep_entries == 0 {
            format!("Deleted {removed} undo history entries")
        } else {
            format!("Trimmed {removed} undo history entries; kept up to {keep_entries} per stack")
        };
        Ok(self.history_result(summary))
    }

    pub fn discard_working_copy(
        &mut self,
        expected_revision: DocumentRevision,
    ) -> Result<HistoryActionResult, RevisionError> {
        self.check_revision(expected_revision)?;
        if let Some(base) = &self.working_base {
            self.design.clone_from(base);
        }
        self.validation = self.calculate_validation();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.revision.0 += 1;
        Ok(self.history_result("Discarded working-copy changes".into()))
    }

    /// Renames the project. The name is document metadata, not part of the
    /// design, so it is not an undo step; the revision still advances so the
    /// working copy reads as unsaved and stale writers are rejected.
    pub fn rename(
        &mut self,
        expected_revision: DocumentRevision,
        name: &str,
    ) -> Result<String, RevisionError> {
        self.check_revision(expected_revision)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(RevisionError::BlankProjectName);
        }
        if name != self.name {
            let old = std::mem::replace(&mut self.name, name.to_owned());
            self.revision.0 += 1;
            return Ok(format!("Renamed project {old} to {name}"));
        }
        Ok(format!("Project is already named {name}"))
    }

    pub fn commit(
        &mut self,
        expected_revision: DocumentRevision,
        id: CommitId,
        name: String,
        description: String,
        timestamp_ms: u64,
    ) -> Result<Commit, RevisionError> {
        self.check_revision(expected_revision)?;
        if name.trim().is_empty() {
            return Err(RevisionError::BlankCommitName);
        }
        let commit = Commit {
            id,
            parent: self.head_commit,
            name,
            description,
            timestamp_ms,
            source_revision: self.revision,
            design: self.design.clone(),
            validation: self.validation.clone(),
            changes: self.undo_stack.clone(),
        };
        self.commits.insert(id, commit.clone());
        self.commit_order.push(id);
        self.head_commit = Some(id);
        self.working_base = Some(self.design.clone());
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.revision.0 += 1;
        Ok(commit)
    }

    pub fn restore_commit(
        &mut self,
        expected_revision: DocumentRevision,
        commit_id: CommitId,
        operation_id: OperationId,
        origin: OperationOrigin,
        rationale: Option<String>,
    ) -> Result<HistoryActionResult, RevisionError> {
        self.check_revision(expected_revision)?;
        let commit = self
            .commits
            .get(&commit_id)
            .ok_or(RevisionError::CommitNotFound)?;
        let change = WorkingChange {
            operation_id,
            origin,
            rationale,
            summary: format!("Restored commit {}", commit.name),
            payloads: Vec::new(),
            before: self.design.clone(),
            after: commit.design.clone(),
            before_validation: self.validation.clone(),
            after_validation: commit.validation.clone(),
        };
        self.design.clone_from(&change.after);
        self.validation.clone_from(&change.after_validation);
        self.undo_stack.push(change);
        self.redo_stack.clear();
        self.revision.0 += 1;
        Ok(self.history_result(format!("Restored commit {}", commit.name)))
    }
}
