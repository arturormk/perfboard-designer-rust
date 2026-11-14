use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use perfboard_core::{
    BoardSize, CommitId, ComponentDefinition, ComponentDraft, ComponentDraftOperation,
    ComponentDraftSnapshot, ProjectId, RectMm, Rotation, RoutePos, RouteProposal, Side,
};
use perfboard_project::{
    AssemblyPlan, Board, Commit, DocumentRevision, HistoryActionResult, OperationError,
    OperationOrigin, OperationRequest, OperationResult, ProjectDocument, ProjectSession,
    RevisionError, RouteRequest, RoutingCostConfig, SemanticDiff, SessionChangeEvent, SessionError,
    TransactionRequest, TransactionResult, assembly_plan, evaluate_route, load_project,
    render_design_svg, save_project_atomic, suggest_route,
};
use perfboard_session_ipc::{
    ComponentDocumentation, DocumentationTrust, ProjectRenamed, RenderedDesign, RouteResults,
    SessionApiError, SessionRegistration, SessionRequest, SessionRequestHandler, SessionResponse,
};
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

mod assembly_progress;
mod component_library;
mod component_viewer;
mod library_grants;
mod library_parts;
mod recent_projects;

use assembly_progress::{AssemblyProgress, Checkmarks};
use recent_projects::{RecentProject, RecentProjects};

use component_library::LibraryInputs;
use library_grants::{GrantEntry, LibraryGrant, LibraryGrants};
use perfboard_library::ResolvedLibrary;

use component_viewer::{
    AuthorizedComponent, COMPONENT_VIEWER_EVENT, ComponentViewerSnapshot, ComponentWatch,
    RuntimeComponentLoad,
};

struct SessionState(Arc<Mutex<ProjectSession>>);

struct IpcRegistrationState(#[allow(dead_code)] Arc<SessionRegistration>);

struct ActiveComponentViewer {
    component: AuthorizedComponent,
    generation: u64,
    _watch: Option<ComponentWatch>,
    watcher_error: Option<String>,
    draft: Option<ComponentDraft>,
    draft_base_revision: Option<String>,
}

struct ComponentViewerState {
    active: Mutex<Option<ActiveComponentViewer>>,
    active_generation: Arc<AtomicU64>,
    /// The folder of parts last picked in "Open part folder…".
    picked_folder: Mutex<Option<PathBuf>>,
}

#[derive(Debug, Serialize)]
struct ComponentDraftSaveResult {
    draft: ComponentDraftSnapshot,
    load: component_viewer::RuntimeComponentLoad,
}

impl Default for ComponentViewerState {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
            picked_folder: Mutex::new(None),
            active_generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

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

/// The definitions offered for new placements, in library order.
fn component_definitions(inputs: &LibraryInputs) -> Vec<ComponentDefinition> {
    component_library::current_library(inputs)
        .components
        .into_iter()
        .map(|component| component.definition)
        .collect()
}

struct DesktopSessionHandler {
    session: Arc<Mutex<ProjectSession>>,
    grants: Arc<LibraryGrants>,
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
            SessionRequest::GetProjectView => {
                // MCP's validation report includes library problems, such as a
                // user library that is not granted, next to the design's.
                let (mut view, inputs) = {
                    let session = self.lock()?;
                    (
                        perfboard_project::ProjectView::from_document(session.document()),
                        LibraryInputs::of(&session, &self.grants),
                    )
                };
                view.validation
                    .extend(component_library::library_validation(
                        &component_library::current_library(&inputs),
                    ));
                Ok(SessionResponse::ProjectView(Box::new(view)))
            }
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
            SessionRequest::RenameProject {
                expected_revision,
                name,
            } => {
                let mut session = self.lock()?;
                let event = session
                    .rename(expected_revision, &name)
                    .map_err(|error| session_api_error(&error))?;
                let renamed = ProjectRenamed {
                    revision: event.revision,
                    name: session.document().name.clone(),
                    summary: event.summary.clone(),
                };
                drop(session);
                self.publish(&event);
                Ok(SessionResponse::Renamed(renamed))
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
            SessionRequest::GetComponentLibrary => {
                let inputs = LibraryInputs::of(&*self.lock()?, &self.grants);
                Ok(SessionResponse::Components(component_definitions(&inputs)))
            }
            SessionRequest::GetComponentDocumentation { definition_id } => {
                let inputs = LibraryInputs::of(&*self.lock()?, &self.grants);
                let Some(markdown) = component_library::current_library(&inputs)
                    .components
                    .into_iter()
                    .find(|component| component.definition.id == definition_id)
                    .map(|component| component.documentation.unwrap_or_default())
                else {
                    return Err(SessionApiError::new(
                        "component.not_found",
                        format!("component definition `{definition_id}` was not found"),
                    ));
                };
                Ok(SessionResponse::Documentation(ComponentDocumentation {
                    definition_id,
                    markdown,
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
    pub version: &'static str,
    pub schema_version: u32,
}

#[tauri::command]
fn get_bootstrap_info() -> BootstrapInfo {
    BootstrapInfo {
        app_name: "Perfboard Designer",
        version: env!("PERFBOARD_VERSION"),
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

/// The build order for the open design (ADR 0012).
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_assembly_plan(state: tauri::State<'_, SessionState>) -> Result<AssemblyPlan, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    let document = session.document();
    Ok(assembly_plan(&document.design, document.revision))
}

fn open_project_id_value(state: &SessionState) -> Result<ProjectId, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())
        .map(|session| session.document().project_id)
}

fn open_project_id(state: &SessionState) -> Result<String, String> {
    state
        .0
        .lock()
        .map_err(|error| error.to_string())
        .map(|session| session.document().project_id.to_string())
}

/// Checkmarks of the open project: step ID → fingerprint when it was ticked.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_assembly_progress(
    state: tauri::State<'_, SessionState>,
    progress: tauri::State<'_, AssemblyProgress>,
) -> Result<Checkmarks, String> {
    Ok(progress.get(&open_project_id(&state)?))
}

/// Ticks a step with the fingerprint it had when done, or unticks it.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn set_assembly_step_done(
    state: tauri::State<'_, SessionState>,
    progress: tauri::State<'_, AssemblyProgress>,
    step_id: String,
    fingerprint: Option<String>,
) -> Result<Checkmarks, String> {
    progress
        .set(&open_project_id(&state)?, &step_id, fingerprint.as_deref())
        .map_err(|error| format!("could not save assembly progress: {error}"))
}

/// Forgets every checkmark of the open project.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn reset_assembly_progress(
    state: tauri::State<'_, SessionState>,
    progress: tauri::State<'_, AssemblyProgress>,
) -> Result<Checkmarks, String> {
    progress
        .clear(&open_project_id(&state)?)
        .map_err(|error| format!("could not save assembly progress: {error}"))
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
fn trim_project_undo_history(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    expected_revision: DocumentRevision,
    keep_entries: usize,
) -> Result<HistoryActionResult, CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let (result, event) = session
        .trim_undo_history(expected_revision, keep_entries)
        .map_err(CommandError::from)?;
    publish(&app, &event);
    Ok(result)
}

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
fn rename_project(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    expected_revision: DocumentRevision,
    name: String,
) -> Result<(), CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let event = session
        .rename(expected_revision, &name)
        .map_err(CommandError::from)?;
    publish(&app, &event);
    Ok(())
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

fn library_inputs(state: &SessionState, grants: &LibraryGrants) -> Result<LibraryInputs, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    Ok(LibraryInputs::of(&session, grants))
}

/// The open project's library: its layers, the definitions that won and
/// what they shadow, and diagnostics. Documentation is fetched separately.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_component_library(
    state: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
) -> Result<ResolvedLibrary, String> {
    Ok(component_library::current_library(&library_inputs(
        &state, &grants,
    )?))
}

/// What the Component libraries panel edits: the project's paths (absent
/// means the default) and its user-library grant on this machine.
#[derive(Debug, Serialize)]
struct LibrarySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    component_paths: Option<Vec<String>>,
    /// The project file; absent until the project is saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    project_file: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grant: Option<LibraryGrant>,
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_library_settings(
    state: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
) -> Result<LibrarySettings, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    Ok(LibrarySettings {
        component_paths: session.document().component_paths.clone(),
        project_file: session.file_path().map(Path::to_path_buf),
        grant: grants.get(session.document().project_id),
    })
}

/// Replaces the project's library layers; `None` restores the default. Only
/// the UI can do this: the MCP session has no request for it (ADR 0014).
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn set_component_paths(
    state: tauri::State<'_, SessionState>,
    app: tauri::AppHandle,
    expected_revision: DocumentRevision,
    paths: Option<Vec<String>>,
) -> Result<(), CommandError> {
    let mut session = state.0.lock().map_err(|error| CommandError {
        code: "session.lock_failed",
        message: error.to_string(),
        expected_revision: None,
        actual_revision: None,
    })?;
    let event = session
        .set_component_paths(expected_revision, paths)
        .map_err(CommandError::from)?;
    drop(session);
    publish(&app, &event);
    Ok(())
}

async fn pick_folder(app: &tauri::AppHandle, title: &str) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .pick_folder(move |selection| {
            let _ = sender.send(selection);
        });
    let Some(selection) = receiver
        .await
        .map_err(|error| format!("the folder picker failed: {error}"))?
    else {
        return Ok(None);
    };
    selection
        .into_path()
        .map(Some)
        .map_err(|error| format!("the selected folder is unavailable: {error}"))
}

/// Asks for a folder and grants it to the open project as its user library.
/// Returns `None` when the picker is cancelled.
#[tauri::command]
async fn grant_user_library(
    state: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    app: tauri::AppHandle,
) -> Result<Option<LibraryGrant>, String> {
    let Some(folder) = pick_folder(&app, "Choose your user component library").await? else {
        return Ok(None);
    };
    let (project_id, name, file) = {
        let session = state.0.lock().map_err(|error| error.to_string())?;
        (
            session.document().project_id,
            session.document().name.clone(),
            session.file_path().map(Path::to_path_buf),
        )
    };
    grants
        .grant(project_id, &folder, &name, file.as_deref())
        .map(Some)
        .map_err(|error| format!("could not grant {}: {error}", folder.display()))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn revoke_user_library(
    state: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
) -> Result<(), String> {
    let project_id = open_project_id_value(&state)?;
    grants
        .revoke(project_id)
        .map_err(|error| format!("could not revoke the grant: {error}"))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn list_library_grants(grants: tauri::State<'_, Arc<LibraryGrants>>) -> Vec<GrantEntry> {
    grants.list()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn revoke_library_grant(
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    project_id: ProjectId,
) -> Result<Vec<GrantEntry>, String> {
    grants
        .revoke(project_id)
        .map_err(|error| format!("could not revoke the grant: {error}"))?;
    Ok(grants.list())
}

/// Asks for a folder and returns it as a `component_paths` entry, relative to
/// the project's folder or its granted user library. Returns `None` when the
/// picker is cancelled; the caller adds the entry with `set_component_paths`.
#[tauri::command]
async fn pick_component_layer(
    state: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let inputs = library_inputs(&state, &grants)?;
    let Some(folder) = pick_folder(&app, "Choose a component library folder").await? else {
        return Ok(None);
    };
    let canonical = |path: &Path| std::fs::canonicalize(path).ok();
    let folder =
        canonical(&folder).ok_or_else(|| format!("{} is unavailable", folder.display()))?;
    let project_dir = inputs
        .project_file
        .as_deref()
        .and_then(Path::parent)
        .and_then(canonical);
    let user_components = inputs.user_components.as_deref().and_then(canonical);
    perfboard_library::template_for_folder(
        &folder,
        project_dir.as_deref(),
        user_components.as_deref(),
    )
    .map(|template| Some(template.to_string()))
    .map_err(|error| match error {
        perfboard_library::FolderTemplateError::OutsideRoots => format!(
            "{} is not inside the project's folder{}. Save the project next to its parts, or \
             grant a user library that contains this folder.",
            folder.display(),
            if user_components.is_some() {
                " or the user library granted to it"
            } else {
                ""
            }
        ),
        other @ perfboard_library::FolderTemplateError::Unrepresentable(_) => other.to_string(),
    })
}

/// Makes `authorized` the Part designer's part: the previous part's watcher
/// stops, this one's starts, and `draft` (if any) becomes the open draft.
fn activate_component(
    state: &ComponentViewerState,
    app: &tauri::AppHandle,
    authorized: AuthorizedComponent,
    load: RuntimeComponentLoad,
    draft: Option<ComponentDraft>,
    draft_base_revision: Option<String>,
) -> Result<ComponentViewerSnapshot, String> {
    let generation = state.active_generation.fetch_add(1, Ordering::AcqRel) + 1;
    let previous = state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    drop(previous);

    let watcher_app = app.clone();
    let watch = ComponentWatch::start(
        authorized.clone(),
        generation,
        Arc::clone(&state.active_generation),
        move |event| {
            if let Err(error) = watcher_app.emit(COMPONENT_VIEWER_EVENT, event) {
                eprintln!("Failed to publish {COMPONENT_VIEWER_EVENT}: {error}");
            }
        },
    );
    let (watch, watcher_error, snapshot) = match watch {
        Ok(watch) => (
            Some(watch),
            None,
            ComponentViewerSnapshot::watching(generation, load),
        ),
        Err(error) => {
            let message = format!("could not watch component directory: {error}");
            (
                None,
                Some(message.clone()),
                ComponentViewerSnapshot::watcher_unavailable(generation, load, message),
            )
        }
    };
    *state.active.lock().map_err(|error| error.to_string())? = Some(ActiveComponentViewer {
        component: authorized,
        generation,
        _watch: watch,
        watcher_error,
        draft,
        draft_base_revision,
    });
    Ok(snapshot)
}

/// Opens the part in `directory`, confining its files to `root`.
fn open_component(
    state: &ComponentViewerState,
    app: &tauri::AppHandle,
    root: &Path,
    directory: &Path,
) -> Result<ComponentViewerSnapshot, String> {
    let authorized = AuthorizedComponent::new(root, directory).map_err(|error| error.message)?;
    let load = authorized.load();
    let draft = load.definition.clone().map(|definition| {
        ComponentDraft::new(definition, load.documentation.clone().unwrap_or_default())
    });
    let draft_base_revision = draft.as_ref().map(|_| load.revision.clone());
    activate_component(state, app, authorized, load, draft, draft_base_revision)
}

/// Starts an unsaved draft of a new part in the empty folder `directory`.
fn start_new_part(
    state: &ComponentViewerState,
    app: &tauri::AppHandle,
    root: &Path,
    directory: &Path,
) -> Result<ComponentViewerSnapshot, String> {
    let authorized = AuthorizedComponent::new(root, directory).map_err(|error| error.message)?;
    let disk = authorized.load();
    if !disk
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "component.definition_missing")
    {
        return Err("the selected directory already contains component.toml".into());
    }
    let definition = ComponentDefinition {
        schema_version: perfboard_core::component::COMPONENT_SCHEMA_VERSION,
        kind: perfboard_core::ComponentKind::Standard,
        // `local.` keeps clear of the prefix reserved for the standard library.
        id: "local.new-part".into(),
        definition_version: "0.1.0".into(),
        name: "New part".into(),
        description: String::new(),
        origin_x: 0,
        origin_y: 0,
        body: RectMm {
            x: -1.27,
            y: -1.27,
            width: 2.54,
            height: 2.54,
        },
        allowed_rotations: vec![Rotation::Deg0],
        pins: Vec::new(),
        occupied_holes: Vec::new(),
        keepouts: Vec::new(),
        wire_keepouts: Vec::new(),
        access: Vec::new(),
        documentation: String::new(),
    };
    let draft = ComponentDraft::new_unsaved(definition.clone(), "");
    let load = RuntimeComponentLoad {
        definition: Some(definition),
        documentation: None,
        component_path: disk.component_path,
        revision: disk.revision.clone(),
        diagnostics: Vec::new(),
    };
    activate_component(
        state,
        app,
        authorized,
        load,
        Some(draft),
        Some(disk.revision),
    )
}

/// What the user picked in "Open part folder…": a part, or a folder of parts
/// to choose from.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PickedFolder {
    Part {
        snapshot: Box<ComponentViewerSnapshot>,
    },
    Library {
        path: PathBuf,
        parts: Vec<library_parts::FolderPart>,
    },
}

/// Asks for a folder. A part folder opens; a folder of parts is listed, and
/// remembered so that `open_library_part` may open one of them.
#[tauri::command]
async fn pick_component_folder(
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
) -> Result<Option<PickedFolder>, String> {
    let Some(path) = pick_folder(&app, "Open a part, or a folder of parts").await? else {
        return Ok(None);
    };
    if !path.join("component.toml").exists()
        && let Some(parts) = library_parts::folder_parts(&path)
    {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        *state
            .picked_folder
            .lock()
            .map_err(|error| error.to_string())? = Some(path.clone());
        return Ok(Some(PickedFolder::Library { path, parts }));
    }
    open_component(&state, &app, &path, &path).map(|snapshot| {
        Some(PickedFolder::Part {
            snapshot: Box::new(snapshot),
        })
    })
}

/// Opens a part from one of the open project's editable libraries, or from
/// the folder of parts the user last picked.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn open_library_part(
    session: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
    directory: PathBuf,
) -> Result<ComponentViewerSnapshot, String> {
    let library = component_library::current_library(&library_inputs(&session, &grants)?);
    let picked = state
        .picked_folder
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let root = library_parts::authorized_parent(&library, picked.as_deref(), &directory)?;
    open_component(&state, &app, &root, &directory)
}

/// Copies a part (the version in use) into the project's folder or its user
/// library and returns the new part folder, ready for `open_library_part`.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn copy_part_to_layer(
    session: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    definition_id: String,
    target: perfboard_library::LayerKind,
) -> Result<PathBuf, String> {
    let inputs = library_inputs(&session, &grants)?;
    let library = component_library::current_library(&inputs);
    let component = library
        .get(&definition_id)
        .ok_or_else(|| format!("{definition_id} is not in this project's libraries"))?;
    let layer = library_parts::writable_layer(&library, &inputs, target)?;
    library_parts::copy_part(component, &layer)
}

/// Starts a new part in a fresh folder of the project's folder or its user
/// library.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn new_part_in_layer(
    session: tauri::State<'_, SessionState>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
    target: perfboard_library::LayerKind,
) -> Result<ComponentViewerSnapshot, String> {
    let inputs = library_inputs(&session, &grants)?;
    let library = component_library::current_library(&inputs);
    let layer = library_parts::writable_layer(&library, &inputs, target)?;
    let folder = library_parts::create_part_folder(&layer)?;
    start_new_part(&state, &app, &layer.path, &folder).inspect_err(|_| {
        let _ = std::fs::remove_dir(&folder);
    })
}

/// Starts a new part in an empty folder the user picks anywhere.
#[tauri::command]
async fn new_component_draft(
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
) -> Result<Option<ComponentViewerSnapshot>, String> {
    let Some(path) = pick_folder(&app, "Choose an empty directory for the new component").await?
    else {
        return Ok(None);
    };
    start_new_part(&state, &app, &path, &path).map(Some)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_component_draft(
    state: tauri::State<'_, ComponentViewerState>,
) -> Result<Option<ComponentDraftSnapshot>, String> {
    let active = state.active.lock().map_err(|error| error.to_string())?;
    Ok(active
        .as_ref()
        .and_then(|active| active.draft.as_ref().map(ComponentDraft::snapshot)))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn apply_component_draft_operation(
    state: tauri::State<'_, ComponentViewerState>,
    operation: ComponentDraftOperation,
) -> Result<ComponentDraftSnapshot, String> {
    state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .as_mut()
        .and_then(|active| active.draft.as_mut())
        .ok_or_else(|| "no valid component draft is open".to_owned())?
        .apply(operation)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn undo_component_draft(
    state: tauri::State<'_, ComponentViewerState>,
) -> Result<ComponentDraftSnapshot, String> {
    state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .as_mut()
        .and_then(|active| active.draft.as_mut())
        .map(ComponentDraft::undo)
        .ok_or_else(|| "no valid component draft is open".to_owned())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn redo_component_draft(
    state: tauri::State<'_, ComponentViewerState>,
) -> Result<ComponentDraftSnapshot, String> {
    state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .as_mut()
        .and_then(|active| active.draft.as_mut())
        .map(ComponentDraft::redo)
        .ok_or_else(|| "no valid component draft is open".to_owned())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn save_component_draft(
    state: tauri::State<'_, ComponentViewerState>,
) -> Result<ComponentDraftSaveResult, String> {
    let mut guard = state.active.lock().map_err(|error| error.to_string())?;
    let active = guard
        .as_mut()
        .ok_or_else(|| "no component is open".to_owned())?;
    let draft = active
        .draft
        .as_mut()
        .ok_or_else(|| "no valid component draft is open".to_owned())?;
    let snapshot = draft.snapshot();
    if !snapshot.valid {
        return Err("invalid drafts cannot be saved".into());
    }
    let source = draft.to_toml().map_err(|error| error.to_string())?;
    let load = active
        .component
        .save(
            &source,
            draft.documentation(),
            active.draft_base_revision.as_deref(),
        )
        .map_err(|error| error.to_string())?;
    active.draft_base_revision = Some(load.revision.clone());
    draft.mark_saved();
    Ok(ComponentDraftSaveResult {
        draft: draft.snapshot(),
        load,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn reload_runtime_component(
    state: tauri::State<'_, ComponentViewerState>,
) -> Result<Option<ComponentViewerSnapshot>, String> {
    state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .as_mut()
        .map(|active| {
            let load = active.component.load();
            if load.definition.is_some()
                && active
                    .draft
                    .as_ref()
                    .is_none_or(|draft| !draft.snapshot().dirty)
            {
                active.draft = load.definition.clone().map(|definition| {
                    ComponentDraft::new(definition, load.documentation.clone().unwrap_or_default())
                });
                active.draft_base_revision = Some(load.revision.clone());
            }
            match &active.watcher_error {
                Some(error) => ComponentViewerSnapshot::watcher_unavailable(
                    active.generation,
                    load,
                    error.clone(),
                ),
                None => ComponentViewerSnapshot::watching(active.generation, load),
            }
        })
        .map_or(Ok(None), |snapshot| Ok(Some(snapshot)))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn close_runtime_component(
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let active = state
        .active
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(active) = active {
        state.active_generation.fetch_add(1, Ordering::AcqRel);
        let generation = active.generation;
        drop(active);
        let _ = app.emit(
            COMPONENT_VIEWER_EVENT,
            ComponentViewerSnapshot::stopped(generation),
        );
    }
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_project_view(
    state: tauri::State<'_, SessionState>,
) -> Result<perfboard_project::ProjectView, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    Ok(perfboard_project::ProjectView::from_document(
        session.document(),
    ))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_commit_design(
    state: tauri::State<'_, SessionState>,
    commit_id: perfboard_core::CommitId,
) -> Result<perfboard_project::Design, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    session
        .document()
        .commits
        .get(&commit_id)
        .map(|commit| commit.design.clone())
        .ok_or_else(|| format!("commit {commit_id} does not exist"))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_undo_history_bytes(state: tauri::State<'_, SessionState>) -> Result<usize, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    Ok(perfboard_project::undo_history_bytes(session.document()))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_net_connectivity(
    state: tauri::State<'_, SessionState>,
) -> Result<Vec<perfboard_project::NetConnectivity>, String> {
    let session = state.0.lock().map_err(|error| error.to_string())?;
    Ok(perfboard_project::net_connectivity(
        &session.document().design,
    ))
}

/// Shows a native project file picker and returns the chosen path, if any.
#[tauri::command]
async fn pick_project_path(
    app: tauri::AppHandle,
    save: bool,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let dialog = app
        .dialog()
        .file()
        .add_filter("Perfboard project", &["json"]);
    if save {
        dialog
            .set_title("Save project")
            .set_file_name(suggested_name.unwrap_or_else(|| "untitled.perfboard.json".into()))
            .save_file(move |selection| {
                let _ = sender.send(selection);
            });
    } else {
        dialog
            .set_title("Open project")
            .pick_file(move |selection| {
                let _ = sender.send(selection);
            });
    }
    let selection = receiver
        .await
        .map_err(|error| format!("project file picker failed: {error}"))?;
    selection
        .map(|path| {
            path.into_path()
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(|error| format!("selected project path is unavailable: {error}"))
        })
        .transpose()
}

/// System fonts for PDF text, loaded on first export.
fn pdf_font_options() -> &'static perfboard_print::usvg::Options<'static> {
    static OPTIONS: std::sync::OnceLock<perfboard_print::usvg::Options<'static>> =
        std::sync::OnceLock::new();
    OPTIONS.get_or_init(perfboard_print::font_options)
}

/// Asks where to save, writes page-sized SVGs as a true-scale PDF, and opens
/// it in the system viewer. Returns the saved path, or `None` if cancelled.
#[tauri::command]
async fn export_pdf(
    app: tauri::AppHandle,
    pages: Vec<String>,
    suggested_name: String,
) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("PDF document", &["pdf"])
        .set_title("Export PDF")
        .set_file_name(suggested_name)
        .save_file(move |selection| {
            let _ = sender.send(selection);
        });
    let Some(selection) = receiver
        .await
        .map_err(|error| format!("PDF file picker failed: {error}"))?
    else {
        return Ok(None);
    };
    let mut path = selection
        .into_path()
        .map_err(|error| format!("selected PDF path is unavailable: {error}"))?;
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        path.as_mut_os_string().push(".pdf");
    }
    let target = path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let pdf = perfboard_print::svg_pages_to_pdf(&pages, pdf_font_options())
            .map_err(|error| error.to_string())?;
        let partial = target.with_extension("pdf.partial");
        std::fs::write(&partial, pdf)
            .and_then(|()| std::fs::rename(&partial, &target))
            .map_err(|error| format!("could not write {}: {error}", target.display()))
    })
    .await
    .map_err(|error| format!("PDF export failed: {error}"))??;
    if let Err(error) = app.opener().open_path(path.to_string_lossy(), None::<&str>) {
        eprintln!("Could not open {}: {error}", path.display());
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn save_project(
    state: tauri::State<'_, SessionState>,
    recent: tauri::State<'_, RecentProjects>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    path: PathBuf,
) -> Result<(), String> {
    let mut session = state.0.lock().map_err(|error| error.to_string())?;
    save_project_atomic(session.document(), &path).map_err(|error| error.to_string())?;
    session.set_file_path(path.clone());
    let (project_id, name) = (
        session.document().project_id,
        session.document().name.clone(),
    );
    drop(session);
    grants.touch(project_id, &name, &path);
    remember_recent(&recent, &path);
    Ok(())
}

/// The recent list is a convenience: failing to update it never fails the
/// open or save that triggered it.
fn remember_recent(recent: &RecentProjects, path: &Path) {
    if let Err(error) = recent.record(path) {
        eprintln!("Could not update the recent projects list: {error}");
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn get_recent_projects(recent: tauri::State<'_, RecentProjects>) -> Vec<RecentProject> {
    recent.list()
}

/// Removes one entry, or all of them when `path` is omitted.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn forget_recent_project(
    recent: tauri::State<'_, RecentProjects>,
    path: Option<PathBuf>,
) -> Result<Vec<RecentProject>, String> {
    recent
        .forget(path.as_deref())
        .map_err(|error| format!("could not update the recent projects list: {error}"))
}

/// Adopts paths remembered by older builds, which kept them in the webview.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn import_recent_projects(
    recent: tauri::State<'_, RecentProjects>,
    paths: Vec<PathBuf>,
) -> Result<Vec<RecentProject>, String> {
    recent
        .import(&paths)
        .map_err(|error| format!("could not update the recent projects list: {error}"))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri extracts State by value.
fn open_project(
    state: tauri::State<'_, SessionState>,
    recent: tauri::State<'_, RecentProjects>,
    grants: tauri::State<'_, Arc<LibraryGrants>>,
    app: tauri::AppHandle,
    path: PathBuf,
) -> Result<ProjectDocument, String> {
    let document = load_project(&path).map_err(|error| error.to_string())?;
    remember_recent(&recent, &path);
    grants.touch(document.project_id, &document.name, &path);
    let event = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .replace_with_file(document.clone(), Some(path));
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
        .manage(ComponentViewerState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let (registration, listener) =
                SessionRegistration::bind().map_err(Box::<dyn std::error::Error>::from)?;
            let grants = Arc::new(LibraryGrants::new(app.path().app_config_dir().ok()));
            let handler: Arc<dyn SessionRequestHandler> = Arc::new(DesktopSessionHandler {
                session: Arc::clone(&ipc_session),
                grants: Arc::clone(&grants),
                app: app.handle().clone(),
            });
            app.manage(grants);
            let task_registration = Arc::clone(&registration);
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    perfboard_session_ipc::serve(listener, task_registration, handler).await
                {
                    eprintln!("Perfboard MCP session host stopped: {error}");
                }
            });
            app.manage(IpcRegistrationState(registration));
            app.manage(RecentProjects::new(app.path().app_config_dir().ok()));
            app.manage(AssemblyProgress::new(app.path().app_config_dir().ok()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap_info,
            get_project,
            get_project_revision,
            get_assembly_plan,
            get_assembly_progress,
            set_assembly_step_done,
            reset_assembly_progress,
            create_project,
            apply_project_operation,
            apply_project_transaction,
            undo_project,
            redo_project,
            trim_project_undo_history,
            discard_project_changes,
            commit_project,
            rename_project,
            restore_project_commit,
            compare_project_revisions,
            suggest_project_route,
            evaluate_project_route,
            get_component_library,
            get_library_settings,
            set_component_paths,
            grant_user_library,
            revoke_user_library,
            list_library_grants,
            revoke_library_grant,
            pick_component_layer,
            pick_component_folder,
            open_library_part,
            copy_part_to_layer,
            new_part_in_layer,
            new_component_draft,
            reload_runtime_component,
            close_runtime_component,
            get_component_draft,
            apply_component_draft_operation,
            undo_component_draft,
            redo_component_draft,
            save_component_draft,
            save_project,
            open_project,
            get_recent_projects,
            forget_recent_project,
            import_recent_projects,
            get_net_connectivity,
            pick_project_path,
            export_pdf,
            get_project_view,
            get_commit_design,
            get_undo_history_bytes,
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
                version: env!("PERFBOARD_VERSION"),
                schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0
            }
        );
    }

    #[test]
    fn an_unsaved_project_is_offered_the_standard_library() {
        let session = ProjectSession::new(initial_document());
        let definitions =
            component_definitions(&LibraryInputs::of(&session, &LibraryGrants::new(None)));
        for id in [
            "dev.perfboard.bridge",
            "dev.perfboard.dupont-jumper",
            "dev.perfboard.resistor_3_holes",
            "dev.perfboard.dupont-pin",
        ] {
            assert!(
                definitions.iter().any(|definition| definition.id == id),
                "{id} is missing"
            );
        }
    }
}
