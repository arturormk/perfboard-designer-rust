//! Schema-versioned project documents and deterministic serialization.

pub mod document;
pub mod operations;
pub mod persistence;

pub use document::{
    Board, Design, DocumentError, DocumentRevision, PROJECT_SCHEMA_VERSION, ProjectDocument,
    SchemaVersion,
};
pub use operations::{
    OperationError, OperationOrigin, OperationPayload, OperationRequest, OperationResult,
    apply_operation,
};
pub use persistence::{PersistenceError, load_project, save_project_atomic};

/// Serialize values in the canonical human-readable snapshot format.
///
/// Stable snapshots use pretty JSON, lexicographically ordered map keys (domain
/// maps are `BTreeMap`s), and exactly one trailing newline.
///
/// # Errors
///
/// Returns an error when `value` cannot be represented as JSON.
pub fn to_snapshot_json<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    Ok(json)
}
mod connectivity;
