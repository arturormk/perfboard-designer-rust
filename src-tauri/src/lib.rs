use std::{path::PathBuf, sync::Mutex};

use perfboard_core::{
    BoardSize, CommitId, ComponentDefinition, ProjectId, RoutePos, RouteProposal, Side,
    parse_component_library,
};
use perfboard_project::{
    Board, Commit, DocumentRevision, HistoryActionResult, OperationError, OperationOrigin,
    OperationRequest, OperationResult, ProjectDocument, ProjectSession, RevisionError,
    RouteRequest, RoutingCostConfig, SemanticDiff, SessionChangeEvent, SessionError,
    TransactionRequest, TransactionResult, evaluate_route, load_project, save_project_atomic,
    suggest_route,
};
use serde::Serialize;
use tauri::Emitter;

struct SessionState(Mutex<ProjectSession>);

#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_revision: Option<DocumentRevision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_revision: Option<DocumentRevision>,
}

impl From<SessionError> for CommandError {
    fn from(error: SessionError) -> Self {
        let revisions = match &error {
            SessionError::Operation(OperationError::StaleRevision { expected, actual })
            | SessionError::Revision(RevisionError::StaleRevision { expected, actual }) => {
                (Some(*expected), Some(*actual))
            }
            _ => (None, None),
        };
        Self {
            code: if revisions.0.is_some() {
                "revision.stale"
            } else {
                "session.operation_failed"
            },
            message: error.to_string(),
            expected_revision: revisions.0,
            actual_revision: revisions.1,
        }
    }
}

fn publish(app: &tauri::AppHandle, event: &SessionChangeEvent) {
    let _ = app.emit("project-changed", event);
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct BootstrapInfo {
    pub app_name: &'static str,
    pub schema_version: u32,
}

#[tauri::command]
fn get_bootstrap_info() -> BootstrapInfo {
    BootstrapInfo {
        app_name: "Perfboard Designer",
        schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0,
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_project(state: tauri::State<'_, SessionState>) -> Result<ProjectDocument, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())
        .map(|value| value.document().clone())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn create_project(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    name: String,
    holes_x: u32,
    holes_y: u32,
    pitch_mm: f64,
) -> Result<ProjectDocument, String> {
    let size = BoardSize::new(holes_x, holes_y).map_err(|error| error.to_string())?;
    let document = ProjectDocument::new(
        ProjectId::new(),
        name,
        Board::new(format!("{holes_x}x{holes_y} perfboard"), size, pitch_mm),
    );
    document.validate().map_err(|error| error.to_string())?;
    let event = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .replace(document.clone());
    publish(&app, &event);
    Ok(document)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn apply_project_operation(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    request: OperationRequest,
) -> Result<OperationResult, CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let (result, event) = session.apply(request).map_err(CommandError::from)?;
    publish(&app, &event);
    Ok(result)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn apply_project_transaction(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    request: TransactionRequest,
) -> Result<TransactionResult, CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let (result, event) = session
        .apply_transaction(request)
        .map_err(CommandError::from)?;
    publish(&app, &event);
    Ok(result)
}

macro_rules! history_command {
    ($name:ident, $method:ident, $kind:literal) => {
        #[tauri::command]
        #[allow(clippy::needless_pass_by_value)]
        fn $name(
            state: tauri::State<'_, SessionState>,
            app: tauri::AppHandle,
            expected_revision: DocumentRevision,
        ) -> Result<HistoryActionResult, CommandError> {
            let mut session = state.0.lock().map_err(|error| CommandError {
                code: "session.lock_failed",
                message: error.to_string(),
                expected_revision: None,
                actual_revision: None,
            })?;
            let (result, event) = session
                .$method(expected_revision)
                .map_err(CommandError::from)?;
            publish(&app, &event);
            Ok(result)
        }
    };
}

history_command!(undo_project, undo, "undo");
history_command!(redo_project, redo, "redo");
history_command!(discard_project_changes, discard, "discard");

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn commit_project(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    expected_revision: DocumentRevision,
    name: String,
    description: String,
) -> Result<Commit, CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let (result, event) = session
        .commit(expected_revision, name, description)
        .map_err(CommandError::from)?;
    publish(&app, &event);
    Ok(result)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn restore_project_commit(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    expected_revision: DocumentRevision,
    commit_id: CommitId,
) -> Result<HistoryActionResult, CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let (result, event) = session
        .restore(
            expected_revision,
            commit_id,
            OperationOrigin::User,
            Some("Restored from history".into()),
        )
        .map_err(CommandError::from)?;
    publish(&app, &event);
    Ok(result)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn compare_project_revisions(
    state: tauri::State<'_, SessionState>,
    from: Option<CommitId>,
    to: Option<CommitId>,
) -> Result<SemanticDiff, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .compare(from, to)
        .ok_or_else(|| "commit was not found".into())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn suggest_project_route(
    state: tauri::State<'_, SessionState>,
    request: RouteRequest,
) -> Result<RouteProposal, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    suggest_route(
        &session.document().design,
        &request,
        &RoutingCostConfig::default(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn evaluate_project_route(
    state: tauri::State<'_, SessionState>,
    request: RouteRequest,
    side: Side,
    vertices: Vec<RoutePos>,
) -> Result<RouteProposal, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    Ok(evaluate_route(
        &session.document().design,
        &request,
        side,
        vertices,
        &RoutingCostConfig::default(),
    ))
}

#[tauri::command]
fn get_component_library() -> Result<Vec<ComponentDefinition>, String> {
    parse_component_library([
        include_str!("../../components/arduino-nano-like/component.toml"),
        include_str!("../../components/rs485-breakout/component.toml"),
    ])
    .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn save_project(state: tauri::State<'_, SessionState>, path: PathBuf) -> Result<(), String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    save_project_atomic(session.document(), path).map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn open_project(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    path: PathBuf,
) -> Result<ProjectDocument, String> {
    let document = load_project(path).map_err(|error| error.to_string())?;
    let event = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .replace(document.clone());
    publish(&app, &event);
    Ok(document)
}

fn initial_document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new(),
        "Untitled project",
        Board::new(
            "25x18 perfboard",
            BoardSize::new(25, 18).expect("valid size"),
            2.54,
        ),
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the desktop application and its command bridge.
///
/// # Panics
/// Panics when Tauri cannot initialize or run the desktop event loop.
pub fn run() {
    tauri::Builder::default()
        .manage(SessionState(Mutex::new(ProjectSession::new(
            initial_document(),
        ))))
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_info,
            get_project,
            create_project,
            apply_project_operation,
            apply_project_transaction,
            undo_project,
            redo_project,
            discard_project_changes,
            commit_project,
            restore_project_commit,
            compare_project_revisions,
            suggest_project_route,
            evaluate_project_route,
            get_component_library,
            save_project,
            open_project,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Perfboard Designer");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_command_reports_shared_schema_version() {
        assert_eq!(
            get_bootstrap_info(),
            BootstrapInfo {
                app_name: "Perfboard Designer",
                schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0
            }
        );
    }

    #[test]
    fn bundled_component_library_is_valid() {
        assert_eq!(get_component_library().unwrap().len(), 2);
    }
}
