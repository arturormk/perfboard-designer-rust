use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use perfboard_core::{CommitId, OperationId, ReviewNoteId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Commit, Design, DocumentRevision, HistoryActionResult, NewReviewNote, OperationError,
    OperationOrigin, OperationPayload, OperationRequest, OperationResult, ProjectDocument,
    ReviewNote, ReviewNoteError, RevisionError, SemanticDiff, WorkingChange,
    operations::apply_operation_untracked,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransactionRequest {
    pub operation_id: OperationId,
    pub expected_revision: DocumentRevision,
    pub origin: OperationOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default)]
    pub allow_invalid_placement: bool,
    pub operations: Vec<OperationPayload>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransactionResult {
    pub operation_id: OperationId,
    pub revision: DocumentRevision,
    pub results: Vec<OperationResult>,
    pub change_summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionChangeEvent {
    pub revision: DocumentRevision,
    pub kind: String,
    pub summary: String,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Operation(#[from] OperationError),
    #[error(transparent)]
    Revision(#[from] RevisionError),
    #[error("a transaction must contain at least one operation")]
    EmptyTransaction,
    #[error(transparent)]
    ReviewNote(#[from] ReviewNoteError),
}

/// Identifies the saved content of the project: its revision plus how many
/// review-note changes the session has made, since note writes leave the
/// revision alone (ADR 0015).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChangeMarker {
    pub revision: DocumentRevision,
    pub notes_generation: u64,
}

#[derive(Clone, Debug)]
pub struct ProjectSession {
    document: ProjectDocument,
    /// The file the project was last opened from or saved to; `None` for a
    /// project that has never been saved.
    file_path: Option<PathBuf>,
    /// Counts review-note changes. In memory only; never reset, so a marker
    /// from one project cannot match another's.
    notes_generation: u64,
    /// The marker when the project was last saved, opened or created.
    saved: ChangeMarker,
}

#[allow(clippy::missing_errors_doc)] // The session consistently wraps typed operation/revision errors.
impl ProjectSession {
    #[must_use]
    pub const fn new(document: ProjectDocument) -> Self {
        let saved = ChangeMarker {
            revision: document.revision,
            notes_generation: 0,
        };
        Self {
            document,
            file_path: None,
            notes_generation: 0,
            saved,
        }
    }

    /// What identifies the current content; it changes with every write.
    #[must_use]
    pub const fn marker(&self) -> ChangeMarker {
        ChangeMarker {
            revision: self.document.revision,
            notes_generation: self.notes_generation,
        }
    }

    /// Whether anything changed since the project was last saved, opened or
    /// created: design edits, history actions, renames, library paths and
    /// review notes alike.
    #[must_use]
    pub fn has_unsaved_changes(&self) -> bool {
        self.marker() != self.saved
    }

    /// Records that the current content was written to `path`.
    pub fn mark_saved(&mut self, path: PathBuf) {
        self.file_path = Some(path);
        self.saved = self.marker();
    }

    /// The editor's read model, including save state.
    #[must_use]
    pub fn view(&self) -> crate::ProjectView {
        let mut view = crate::ProjectView::from_document(&self.document);
        view.notes_generation = self.notes_generation;
        view.unsaved = self.has_unsaved_changes();
        view
    }

    /// The file the project was last opened from or saved to.
    #[must_use]
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Sets the project's file without changing its save state; see
    /// [`Self::mark_saved`] for recording a save.
    pub fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }

    #[must_use]
    pub const fn document(&self) -> &ProjectDocument {
        &self.document
    }

    /// Replaces the project with one that has no file yet.
    pub fn replace(&mut self, document: ProjectDocument) -> SessionChangeEvent {
        self.replace_with_file(document, None)
    }

    /// Replaces the project with one opened from `file_path`.
    pub fn replace_with_file(
        &mut self,
        document: ProjectDocument,
        file_path: Option<PathBuf>,
    ) -> SessionChangeEvent {
        self.document = document;
        self.file_path = file_path;
        self.saved = self.marker();
        self.event("project_replaced", "Replaced active project".into())
    }

    pub fn apply(
        &mut self,
        request: OperationRequest,
    ) -> Result<(OperationResult, SessionChangeEvent), SessionError> {
        let result = crate::apply_operation(&mut self.document, request)?;
        let event = self.event("operation", result.change_summary.clone());
        Ok((result, event))
    }

    pub fn apply_transaction(
        &mut self,
        request: TransactionRequest,
    ) -> Result<(TransactionResult, SessionChangeEvent), SessionError> {
        if request.operations.is_empty() {
            return Err(SessionError::EmptyTransaction);
        }
        if request.expected_revision != self.document.revision {
            return Err(OperationError::StaleRevision {
                expected: request.expected_revision,
                actual: self.document.revision,
            }
            .into());
        }
        let before = self.document.design.clone();
        let before_validation = self.document.validation.clone();
        let mut candidate = self.document.clone();
        if candidate.working_base.is_none() {
            candidate.working_base = Some(before.clone());
        }
        let mut results = Vec::with_capacity(request.operations.len());
        for payload in &request.operations {
            let expected_revision = candidate.revision;
            results.push(apply_operation_untracked(
                &mut candidate,
                OperationRequest {
                    operation_id: request.operation_id,
                    expected_revision,
                    origin: request.origin,
                    rationale: request.rationale.clone(),
                    allow_invalid_placement: request.allow_invalid_placement,
                    payload: payload.clone(),
                },
            )?);
        }
        let summary = results
            .iter()
            .map(|result| result.change_summary.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        candidate.undo_stack.push(WorkingChange {
            operation_id: request.operation_id,
            origin: request.origin,
            rationale: request.rationale,
            summary: summary.clone(),
            payloads: request.operations,
            before,
            after: candidate.design.clone(),
            before_validation,
            after_validation: candidate.validation.clone(),
        });
        candidate.redo_stack.clear();
        self.document = candidate;
        let result = TransactionResult {
            operation_id: request.operation_id,
            revision: self.document.revision,
            results,
            change_summary: summary.clone(),
        };
        Ok((result, self.event("transaction", summary)))
    }

    pub fn undo(
        &mut self,
        expected: DocumentRevision,
    ) -> Result<(HistoryActionResult, SessionChangeEvent), SessionError> {
        let result = self.document.undo(expected)?;
        Ok((result.clone(), self.event("undo", result.summary.clone())))
    }

    pub fn redo(
        &mut self,
        expected: DocumentRevision,
    ) -> Result<(HistoryActionResult, SessionChangeEvent), SessionError> {
        let result = self.document.redo(expected)?;
        Ok((result.clone(), self.event("redo", result.summary.clone())))
    }

    pub fn trim_undo_history(
        &mut self,
        expected: DocumentRevision,
        keep_entries: usize,
    ) -> Result<(HistoryActionResult, SessionChangeEvent), SessionError> {
        let result = self.document.trim_undo_history(expected, keep_entries)?;
        Ok((
            result.clone(),
            self.event("history_trimmed", result.summary.clone()),
        ))
    }

    pub fn discard(
        &mut self,
        expected: DocumentRevision,
    ) -> Result<(HistoryActionResult, SessionChangeEvent), SessionError> {
        let result = self.document.discard_working_copy(expected)?;
        Ok((
            result.clone(),
            self.event("discard", result.summary.clone()),
        ))
    }

    pub fn commit(
        &mut self,
        expected: DocumentRevision,
        name: String,
        description: String,
    ) -> Result<(Commit, SessionChangeEvent), SessionError> {
        let commit =
            self.document
                .commit(expected, CommitId::new(), name, description, now_ms())?;
        Ok((
            commit.clone(),
            self.event("commit", format!("Committed {}", commit.name)),
        ))
    }

    pub fn rename(
        &mut self,
        expected: DocumentRevision,
        name: &str,
    ) -> Result<SessionChangeEvent, SessionError> {
        let summary = self.document.rename(expected, name)?;
        Ok(self.event("rename", summary))
    }

    pub fn set_component_paths(
        &mut self,
        expected: DocumentRevision,
        paths: Option<Vec<String>>,
    ) -> Result<SessionChangeEvent, SessionError> {
        let summary = self.document.set_component_paths(expected, paths)?;
        Ok(self.event("component_paths", summary))
    }

    /// Appends review notes (ADR 0015). Needs no expected revision, since
    /// notes cannot conflict with design edits, and leaves the revision as
    /// it is; the change still counts as unsaved.
    pub fn add_review_notes(
        &mut self,
        notes: Vec<NewReviewNote>,
    ) -> Result<(Vec<ReviewNote>, SessionChangeEvent), SessionError> {
        let added = self.document.add_review_notes(notes, now_ms())?;
        self.notes_generation += 1;
        let summary = match added.as_slice() {
            [note] => format!("Added review note “{}”", note.title),
            notes => format!("Added {} review notes", notes.len()),
        };
        Ok((added, self.event("review_notes", summary)))
    }

    pub fn remove_review_notes(
        &mut self,
        ids: &[ReviewNoteId],
    ) -> Result<(Vec<ReviewNote>, SessionChangeEvent), SessionError> {
        let removed = self.document.remove_review_notes(ids)?;
        self.notes_generation += 1;
        let summary = match removed.as_slice() {
            [note] => format!("Removed review note “{}”", note.title),
            notes => format!("Removed {} review notes", notes.len()),
        };
        Ok((removed, self.event("review_notes", summary)))
    }

    pub fn move_review_note(
        &mut self,
        id: ReviewNoteId,
        before: Option<ReviewNoteId>,
    ) -> Result<SessionChangeEvent, SessionError> {
        let summary = if self.document.move_review_note(id, before)? {
            self.notes_generation += 1;
            "Reordered review notes"
        } else {
            "Review note order is unchanged"
        };
        Ok(self.event("review_notes", summary.into()))
    }

    pub fn insert_review_note(
        &mut self,
        note: ReviewNote,
        index: usize,
    ) -> Result<SessionChangeEvent, SessionError> {
        let summary = format!("Restored review note “{}”", note.title);
        self.document.insert_review_note(note, index)?;
        self.notes_generation += 1;
        Ok(self.event("review_notes", summary))
    }

    pub fn restore(
        &mut self,
        expected: DocumentRevision,
        commit_id: CommitId,
        origin: OperationOrigin,
        rationale: Option<String>,
    ) -> Result<(HistoryActionResult, SessionChangeEvent), SessionError> {
        let result = self.document.restore_commit(
            expected,
            commit_id,
            OperationId::new(),
            origin,
            rationale,
        )?;
        Ok((
            result.clone(),
            self.event("restore", result.summary.clone()),
        ))
    }

    #[must_use]
    pub fn compare(&self, from: Option<CommitId>, to: Option<CommitId>) -> Option<SemanticDiff> {
        let resolve = |reference: Option<CommitId>| -> Option<&Design> {
            match reference {
                Some(id) => self.document.commits.get(&id).map(|commit| &commit.design),
                None => Some(&self.document.design),
            }
        };
        Some(SemanticDiff::between(resolve(from)?, resolve(to)?))
    }

    fn event(&self, kind: &str, summary: String) -> SessionChangeEvent {
        SessionChangeEvent {
            revision: self.document.revision,
            kind: kind.into(),
            summary,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
