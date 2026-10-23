use std::time::{SystemTime, UNIX_EPOCH};

use perfboard_core::{CommitId, OperationId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Commit, Design, DocumentRevision, HistoryActionResult, OperationError, OperationOrigin,
    OperationPayload, OperationRequest, OperationResult, ProjectDocument, RevisionError,
    SemanticDiff, WorkingChange, operations::apply_operation_untracked,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionResult {
    pub operation_id: OperationId,
    pub revision: DocumentRevision,
    pub results: Vec<OperationResult>,
    pub change_summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug)]
pub struct ProjectSession {
    document: ProjectDocument,
}

#[allow(clippy::missing_errors_doc)] // The session consistently wraps typed operation/revision errors.
impl ProjectSession {
    #[must_use]
    pub const fn new(document: ProjectDocument) -> Self {
        Self { document }
    }

    #[must_use]
    pub const fn document(&self) -> &ProjectDocument {
        &self.document
    }

    pub fn replace(&mut self, document: ProjectDocument) -> SessionChangeEvent {
        self.document = document;
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
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let commit =
            self.document
                .commit(expected, CommitId::new(), name, description, timestamp_ms)?;
        Ok((
            commit.clone(),
            self.event("commit", format!("Committed {}", commit.name)),
        ))
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
