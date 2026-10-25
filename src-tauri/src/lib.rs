use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use perfboard_core::{
    BoardSize, CommitId, ComponentDefinition, ProjectId, RoutePos, RouteProposal, Side,
    parse_component_library,
};
use perfboard_project::{
    Board, Commit, DocumentRevision, HistoryActionResult, OperationError, OperationOrigin,
    OperationRequest, OperationResult, ProjectDocument, ProjectSession, RevisionError,
    RouteRequest, RoutingCostConfig, SemanticDiff, SessionChangeEvent, SessionError,
    TransactionRequest, TransactionResult, evaluate_route, load_project, render_design_svg,
    save_project_atomic, suggest_route,
};
use perfboard_session_ipc::{
    ComponentDocumentation, DocumentationTrust, RenderedDesign, RouteResults, SessionApiError,
    SessionRegistration, SessionRequest, SessionRequestHandler, SessionResponse,
};
use serde::Serialize;
use tauri::{Emitter, Manager};

struct SessionState(Arc<Mutex<ProjectSession>>);

struct IpcRegistrationState(#[allow(dead_code)] Arc<SessionRegistration>);

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
    if let Err(error) = app.emit("project-changed", event) {
        eprintln!(
            "Failed to publish project-changed for revision {}: {error}",
            event.revision.0
        );
    }
}

fn session_api_error(error: &SessionError) -> SessionApiError {
    let revisions = match &error {
        SessionError::Operation(OperationError::StaleRevision { expected, actual })
        | SessionError::Revision(RevisionError::StaleRevision { expected, actual }) => {
            (Some(*expected), Some(*actual))
        }
        _ => (None, None),
    };
    SessionApiError {
        code: if revisions.0.is_some() {
            "revision.stale".into()
        } else {
            "session.operation_failed".into()
        },
        message: error.to_string(),
        expected_revision: revisions.0,
        actual_revision: revisions.1,
        retryable: revisions.0.is_some(),
    }
}

fn bundled_component_library() -> Result<Vec<ComponentDefinition>, String> {
    parse_component_library([
        include_str!("../../components/arduino-nano-like/component.toml"),
        include_str!("../../components/rs485-breakout/component.toml"),
    ])
    .map_err(|error| error.to_string())
}

struct DesktopSessionHandler {
    session: Arc<Mutex<ProjectSession>>,
    app: tauri::AppHandle,
}

impl DesktopSessionHandler {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ProjectSession>, SessionApiError> {
        self.session.lock().map_err(|error| SessionApiError {
            code: "session.lock_failed".into(),
            message: error.to_string(),
            expected_revision: None,
            actual_revision: None,
            retryable: true,
        })
    }

    fn publish(&self, event: &SessionChangeEvent) {
        publish(&self.app, event);
    }
}

impl SessionRequestHandler for DesktopSessionHandler {
    #[allow(clippy::too_many_lines)] // This is the transport-to-session dispatch table.
    fn handle(&self, request: SessionRequest) -> Result<SessionResponse, SessionApiError> {
        match request {
            SessionRequest::Ping => {
                let session = self.lock()?;
                Ok(SessionResponse::Pong {
                    revision: session.document().revision,
                    project_name: session.document().name.clone(),
                })
            }
            SessionRequest::GetProject => Ok(SessionResponse::Project(Box::new(
                self.lock()?.document().clone(),
            ))),
            SessionRequest::ApplyTransaction(request) => {
                let (result, event) = self
                    .lock()?
                    .apply_transaction(request)
                    .map_err(|error| session_api_error(&error))?;
                self.publish(&event);
                Ok(SessionResponse::Transaction(result))
            }
            SessionRequest::Undo { expected_revision } => {
                let (result, event) = self
                    .lock()?
                    .undo(expected_revision)
                    .map_err(|error| session_api_error(&error))?;
                self.publish(&event);
                Ok(SessionResponse::History(result))
            }
            SessionRequest::Redo { expected_revision } => {
                let (result, event) = self
                    .lock()?
                    .redo(expected_revision)
                    .map_err(|error| session_api_error(&error))?;
                self.publish(&event);
                Ok(SessionResponse::History(result))
            }
            SessionRequest::Discard { expected_revision } => {
                let (result, event) = self
                    .lock()?
                    .discard(expected_revision)
                    .map_err(|error| session_api_error(&error))?;
                self.publish(&event);
                Ok(SessionResponse::History(result))
            }
            SessionRequest::Commit {
                expected_revision,
                name,
                description,
            } => {
                let (result, event) = self
                    .lock()?
                    .commit(expected_revision, name, description)
                    .map_err(|error| session_api_error(&error))?;
                self.publish(&event);
                Ok(SessionResponse::Commit(Box::new(result)))
            }
            SessionRequest::Restore {
                expected_revision,
                commit_id,
                rationale,
            } => {
                let (result, event) = self
                    .lock()?
                    .restore(
                        expected_revision,
                        commit_id,
                        OperationOrigin::Mcp,
                        rationale,
                    )
                    .map_err(|error| session_api_error(&error))?;
                self.publish(&event);
                Ok(SessionResponse::History(result))
            }
            SessionRequest::Compare { from, to } => self
                .lock()?
                .compare(from, to)
                .map(SessionResponse::Diff)
                .ok_or_else(|| SessionApiError::new("revision.not_found", "commit was not found")),
            SessionRequest::SuggestRoutes { requests } => {
                let session = self.lock()?;
                let proposals = requests
                    .iter()
                    .map(|request| {
                        suggest_route(
                            &session.document().design,
                            request,
                            &RoutingCostConfig::default(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| SessionApiError::new("route.not_found", error.to_string()))?;
                Ok(SessionResponse::Routes(RouteResults {
                    revision: session.document().revision,
                    proposals,
                }))
            }
            SessionRequest::EvaluateRoute {
                request,
                side,
                vertices,
            } => {
                let session = self.lock()?;
                Ok(SessionResponse::Routes(RouteResults {
                    revision: session.document().revision,
                    proposals: vec![evaluate_route(
                        &session.document().design,
                        &request,
                        side,
                        vertices,
                        &RoutingCostConfig::default(),
                    )],
                }))
            }
            SessionRequest::GetComponentLibrary => bundled_component_library()
                .map(SessionResponse::Components)
                .map_err(|message| SessionApiError::new("component.library_invalid", message)),
            SessionRequest::GetComponentDocumentation { definition_id } => {
                let markdown = match definition_id.as_str() {
                    "dev.perfboard.nano-like" => {
                        include_str!("../../components/arduino-nano-like/README.md")
                    }
                    "dev.perfboard.rs485-breakout" => {
                        include_str!("../../components/rs485-breakout/README.md")
                    }
                    _ => {
                        return Err(SessionApiError::new(
                            "component.not_found",
                            format!("component definition `{definition_id}` was not found"),
                        ));
                    }
                };
                Ok(SessionResponse::Documentation(ComponentDocumentation {
                    definition_id,
                    markdown: markdown.into(),
                    trust: DocumentationTrust::UntrustedReference,
                }))
            }
            SessionRequest::RenderDesign { side } => {
                let session = self.lock()?;
                Ok(SessionResponse::Rendered(RenderedDesign {
                    revision: session.document().revision,
                    side,
                    mime_type: "image/svg+xml".into(),
                    svg: render_design_svg(session.document(), side),
                }))
            }
        }
    }
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
fn get_project_revision(state: tauri::State<'_, SessionState>) -> Result<DocumentRevision, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())
        .map(|value| value.document().revision)
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
    bundled_component_library()
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
    let session = Arc::new(Mutex::new(ProjectSession::new(initial_document())));
    let ipc_session = Arc::clone(&session);
    tauri::Builder::default()
        .manage(SessionState(session))
        .setup(move |app| {
            let (registration, listener) =
                SessionRegistration::bind().map_err(Box::<dyn std::error::Error>::from)?;
            let handler: Arc<dyn SessionRequestHandler> = Arc::new(DesktopSessionHandler {
                session: Arc::clone(&ipc_session),
                app: app.handle().clone(),
            });
            let task_registration = Arc::clone(&registration);
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    perfboard_session_ipc::serve(listener, task_registration, handler).await
                {
                    eprintln!("Perfboard MCP session host stopped: {error}");
                }
            });
            app.manage(IpcRegistrationState(registration));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_info,
            get_project,
            get_project_revision,
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
