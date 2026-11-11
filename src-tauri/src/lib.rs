use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use perfboard_core::{
    BoardSize, CommitId, ComponentDefinition, ComponentDraft, ComponentDraftOperation,
    ComponentDraftSnapshot, ProjectId, RectMm, Rotation, RoutePos, RouteProposal, Side,
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
use tauri_plugin_dialog::DialogExt;

mod component_viewer;

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

fn bundled_component_library() -> Result<Vec<ComponentDefinition>, String> {
    parse_component_library([
        include_str!("../../components/bridge/component.toml"),
        include_str!("../../components/arduino-nano-like/component.toml"),
        include_str!("../../components/rs485-breakout/component.toml"),
    ])
    .map_err(|error| error.to_string())
}

fn load_component_library(root: &Path) -> Result<Vec<ComponentDefinition>, String> {
    let entries = fs::read_dir(root).map_err(|error| {
        format!(
            "could not read component library {}: {error}",
            root.display()
        )
    })?;
    let mut definition_paths = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("component.toml"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    definition_paths.sort();

    let sources = definition_paths
        .iter()
        .map(|path| {
            fs::read_to_string(path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    parse_component_library(sources.iter().map(String::as_str)).map_err(|error| error.to_string())
}

fn component_library_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../components")
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
                    "dev.perfboard.bridge" => include_str!("../../components/bridge/README.md"),
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
    load_component_library(&component_library_root())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn select_runtime_component(
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
) -> Result<Option<ComponentViewerSnapshot>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Open component directory")
        .pick_folder(move |selection| {
            let _ = sender.send(selection);
        });
    let Some(selection) = receiver
        .await
        .map_err(|error| format!("component directory picker failed: {error}"))?
    else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|error| format!("selected component directory is unavailable: {error}"))?;
    let authorized = AuthorizedComponent::new(&path, &path).map_err(|error| error.message)?;
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
    let load = authorized.load();
    let draft = load.definition.clone().map(|definition| {
        ComponentDraft::new(definition, load.documentation.clone().unwrap_or_default())
    });
    let draft_base_revision = draft.as_ref().map(|_| load.revision.clone());
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
    Ok(Some(snapshot))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
async fn new_component_draft(
    state: tauri::State<'_, ComponentViewerState>,
    app: tauri::AppHandle,
) -> Result<Option<ComponentViewerSnapshot>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose an empty directory for the new component")
        .pick_folder(move |selection| {
            let _ = sender.send(selection);
        });
    let Some(selection) = receiver
        .await
        .map_err(|error| format!("component directory picker failed: {error}"))?
    else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|error| format!("selected component directory is unavailable: {error}"))?;
    let authorized = AuthorizedComponent::new(&path, &path).map_err(|error| error.message)?;
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
        id: "dev.example.new-component".into(),
        definition_version: "0.1.0".into(),
        name: "New component".into(),
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
    let load = RuntimeComponentLoad {
        definition: Some(definition),
        documentation: None,
        component_path: disk.component_path,
        revision: disk.revision.clone(),
        diagnostics: Vec::new(),
    };
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
        draft: Some(draft),
        draft_base_revision: Some(disk.revision),
    });
    Ok(Some(snapshot))
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
        .manage(ComponentViewerState::default())
        .plugin(tauri_plugin_dialog::init())
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
            trim_project_undo_history,
            discard_project_changes,
            commit_project,
            restore_project_commit,
            compare_project_revisions,
            suggest_project_route,
            evaluate_project_route,
            get_component_library,
            select_runtime_component,
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
            get_net_connectivity,
            pick_project_path,
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
                schema_version: perfboard_project::PROJECT_SCHEMA_VERSION.0
            }
        );
    }

    #[test]
    fn bundled_component_library_is_valid() {
        assert_eq!(bundled_component_library().unwrap().len(), 3);
    }

    #[test]
    fn runtime_component_library_scans_component_directories() {
        let library = get_component_library().unwrap();
        assert!(library.len() >= 3);
        assert!(
            library
                .iter()
                .any(|definition| definition.id == "dev.perfboard.resistor_3_holes")
        );
        assert!(
            library
                .iter()
                .any(|definition| definition.id == "dev.perfboard.resistor_4_holes")
        );
        assert!(
            library
                .iter()
                .any(|definition| definition.id == "dev.perfboard.rj45-connector")
        );
        assert!(
            library
                .iter()
                .any(|definition| definition.id == "dev.perfboard.dupont-jumper")
        );
        assert!(
            library
                .iter()
                .any(|definition| definition.id == "dev.perfboard.bridge")
        );
    }
}
