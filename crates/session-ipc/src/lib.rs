//! Authenticated, local-only IPC between the desktop session and adapters.

use std::{
    env, fs, io,
    net::{Ipv4Addr, SocketAddrV4, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use directories::{BaseDirs, ProjectDirs};
use perfboard_core::{CommitId, ComponentDefinition, ReviewNoteId, RoutePos, RouteProposal, Side};
use perfboard_project::{
    Commit, DocumentRevision, HistoryActionResult, NewReviewNote, ProjectDocument, ProjectView,
    ReviewNote, ReviewNoteUpdate, RouteAlternative, RouteRequest, SemanticDiff, TransactionRequest,
    TransactionResult,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use uuid::Uuid;

pub const SESSION_PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub const SESSION_SELECTOR_ENV: &str = "PERFBOARD_DESIGNER_SESSION_ID";
pub const RUNTIME_DIRECTORY_ENV: &str = "PERFBOARD_DESIGNER_RUNTIME_DIR";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum SessionRequest {
    Ping,
    /// The whole document, including every undo/redo and commit snapshot.
    /// It can outgrow the frame limit; prefer `GetProjectView`.
    GetProject,
    /// The current design, validation and compact history summaries.
    GetProjectView,
    ApplyTransaction(TransactionRequest),
    Undo {
        expected_revision: DocumentRevision,
    },
    Redo {
        expected_revision: DocumentRevision,
    },
    Discard {
        expected_revision: DocumentRevision,
    },
    Commit {
        expected_revision: DocumentRevision,
        name: String,
        description: String,
    },
    /// Renames the project. Not an undo step; advances the revision.
    RenameProject {
        expected_revision: DocumentRevision,
        name: String,
    },
    /// Adds review notes (ADR 0015), each in front of the first less severe
    /// note. Needs no expected revision and leaves the revision alone.
    AddReviewNotes {
        notes: Vec<NewReviewNote>,
    },
    /// Removes review notes by ID. Needs no expected revision.
    RemoveReviewNotes {
        note_ids: Vec<ReviewNoteId>,
    },
    /// Changes a review note's text, severity, status, reply or response.
    /// Needs no expected revision.
    UpdateReviewNote {
        note_id: ReviewNoteId,
        update: ReviewNoteUpdate,
    },
    /// Moves a review note in front of `before`, or to the end when `before`
    /// is absent. Needs no expected revision.
    MoveReviewNote {
        note_id: ReviewNoteId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<ReviewNoteId>,
    },
    Restore {
        expected_revision: DocumentRevision,
        commit_id: CommitId,
        rationale: Option<String>,
    },
    Compare {
        from: Option<CommitId>,
        to: Option<CommitId>,
    },
    SuggestRoutes {
        requests: Vec<RouteRequest>,
        /// Also return up to five distinct alternatives per request.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        alternatives: bool,
    },
    EvaluateRoute {
        request: RouteRequest,
        side: Side,
        vertices: Vec<RoutePos>,
    },
    GetComponentLibrary,
    GetComponentDocumentation {
        definition_id: String,
    },
    RenderDesign {
        side: Side,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SessionResponse {
    Pong {
        revision: DocumentRevision,
        project_name: String,
    },
    Project(Box<ProjectDocument>),
    ProjectView(Box<ProjectView>),
    Transaction(TransactionResult),
    History(HistoryActionResult),
    Commit(Box<Commit>),
    Renamed(ProjectRenamed),
    ReviewNotes(ReviewNotesChanged),
    Diff(SemanticDiff),
    Routes(RouteResults),
    Components(Vec<ComponentDefinition>),
    Documentation(ComponentDocumentation),
    Rendered(RenderedDesign),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectRenamed {
    pub revision: DocumentRevision,
    pub name: String,
    pub summary: String,
}

/// The notes a review-note request added, removed, moved or updated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewNotesChanged {
    pub revision: DocumentRevision,
    pub notes: Vec<ReviewNote>,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentDocumentation {
    pub definition_id: String,
    pub markdown: String,
    pub trust: DocumentationTrust,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentationTrust {
    UntrustedReference,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RenderedDesign {
    pub revision: DocumentRevision,
    pub side: Side,
    pub mime_type: String,
    pub svg: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteResults {
    pub revision: DocumentRevision,
    pub proposals: Vec<RouteProposal>,
    /// Per request, when alternatives were asked for; the first of each is
    /// the proposal above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<Vec<RouteAlternative>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<DocumentRevision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_revision: Option<DocumentRevision>,
    pub retryable: bool,
}

impl SessionApiError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            expected_revision: None,
            actual_revision: None,
            retryable: false,
        }
    }
}

pub trait SessionRequestHandler: Send + Sync + 'static {
    /// Handles one authenticated session request.
    ///
    /// # Errors
    ///
    /// Returns a structured API error when the request cannot be completed.
    fn handle(&self, request: SessionRequest) -> Result<SessionResponse, SessionApiError>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionDescriptor {
    protocol_version: u32,
    session_id: String,
    pid: u32,
    address: SocketAddrV4,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RequestEnvelope {
    protocol_version: u32,
    token: String,
    request: SessionRequest,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseEnvelope {
    protocol_version: u32,
    result: Result<SessionResponse, SessionApiError>,
}

#[derive(Debug)]
pub struct SessionRegistration {
    descriptor_path: PathBuf,
    descriptor: SessionDescriptor,
}

impl SessionRegistration {
    /// Registers a new session and returns its listener.
    ///
    /// # Errors
    ///
    /// Returns an IPC error if the runtime directory or loopback listener cannot be created.
    pub fn bind() -> Result<(Arc<Self>, StdTcpListener), IpcError> {
        Self::bind_in(&session_directory()?)
    }

    /// Registers a session in an explicit directory, primarily for isolated hosts and tests.
    ///
    /// # Errors
    ///
    /// Returns an IPC error if the directory, descriptor, or loopback listener cannot be created.
    pub fn bind_in(directory: &Path) -> Result<(Arc<Self>, StdTcpListener), IpcError> {
        let listener = StdTcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        fs::create_dir_all(directory)?;
        restrict_directory(directory)?;
        let session_id = Uuid::new_v4().to_string();
        let descriptor = SessionDescriptor {
            protocol_version: SESSION_PROTOCOL_VERSION,
            session_id: session_id.clone(),
            pid: std::process::id(),
            address: match listener.local_addr()? {
                std::net::SocketAddr::V4(address) => address,
                std::net::SocketAddr::V6(_) => return Err(IpcError::UnexpectedIpv6),
            },
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        };
        let descriptor_path = directory.join(format!("session-{session_id}.json"));
        let temporary = directory.join(format!(".session-{session_id}.tmp"));
        fs::write(&temporary, serde_json::to_vec(&descriptor)?)?;
        restrict_file(&temporary)?;
        fs::rename(&temporary, &descriptor_path)?;
        Ok((
            Arc::new(Self {
                descriptor_path,
                descriptor,
            }),
            listener,
        ))
    }

    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.descriptor.session_id
    }
}

impl Drop for SessionRegistration {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.descriptor_path);
    }
}

/// Serves authenticated one-request connections until the listener closes.
///
/// # Errors
///
/// Returns an IPC error if the listener cannot start or stops accepting connections.
pub async fn serve(
    listener: StdTcpListener,
    registration: Arc<SessionRegistration>,
    handler: Arc<dyn SessionRequestHandler>,
) -> Result<(), IpcError> {
    let listener = TcpListener::from_std(listener)?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        let registration = Arc::clone(&registration);
        let handler = Arc::clone(&handler);
        tokio::spawn(async move {
            let result = async {
                let envelope: RequestEnvelope = read_frame(&mut stream).await?;
                let result = if envelope.protocol_version != SESSION_PROTOCOL_VERSION {
                    Err(SessionApiError::new(
                        "protocol.unsupported",
                        format!(
                            "session protocol {} is unsupported; expected {}",
                            envelope.protocol_version, SESSION_PROTOCOL_VERSION
                        ),
                    ))
                } else if envelope.token != registration.descriptor.token {
                    Err(SessionApiError::new(
                        "session.unauthorized",
                        "invalid session authentication token",
                    ))
                } else {
                    handler.handle(envelope.request)
                };
                let response = ResponseEnvelope {
                    protocol_version: SESSION_PROTOCOL_VERSION,
                    result,
                };
                match write_frame(&mut stream, &response).await {
                    Err(IpcError::FrameTooLarge(size)) => {
                        write_frame(&mut stream, &too_large_response(size)).await
                    }
                    other => other,
                }
            }
            .await;
            if let Err(error) = result
                && !matches!(&error, IpcError::Io(io_error) if io_error.kind() == io::ErrorKind::UnexpectedEof)
            {
                eprintln!("Perfboard session IPC error: {error}");
            }
        });
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionClient {
    runtime_directory: Option<PathBuf>,
    session_id: Option<String>,
}

impl SessionClient {
    #[must_use]
    pub fn with_runtime_directory(runtime_directory: PathBuf) -> Self {
        Self {
            runtime_directory: Some(runtime_directory),
            session_id: None,
        }
    }

    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Discovers the desktop session and performs one request.
    ///
    /// # Errors
    ///
    /// Returns an IPC error when discovery, authentication, transport, or the remote request fails.
    pub async fn call(&self, request: SessionRequest) -> Result<SessionResponse, IpcError> {
        let directory = self
            .runtime_directory
            .clone()
            .map_or_else(session_directory, Ok)?;
        let discovery = discover_sessions(&directory).await?;
        let selected = select_descriptor(discovery, self.session_id.as_deref())?;
        let address = std::net::SocketAddr::V4(selected.address);
        let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(|_| IpcError::ConnectionTimeout)??;
        let envelope = RequestEnvelope {
            protocol_version: SESSION_PROTOCOL_VERSION,
            token: selected.token,
            request,
        };
        timeout(REQUEST_TIMEOUT, async {
            write_frame(&mut stream, &envelope).await?;
            let response: ResponseEnvelope = read_frame(&mut stream).await?;
            if response.protocol_version != SESSION_PROTOCOL_VERSION {
                return Err(IpcError::ProtocolMismatch(response.protocol_version));
            }
            response.result.map_err(IpcError::Remote)
        })
        .await
        .map_err(|_| IpcError::RequestTimeout)?
    }
}

#[derive(Debug)]
struct SessionDiscovery {
    live: Vec<SessionDescriptor>,
    unreachable: Vec<UnreachableSession>,
}

#[derive(Clone, Debug)]
pub struct UnreachableSession {
    pub session_id: String,
    pub descriptor_path: PathBuf,
    pub reason: String,
}

async fn discover_sessions(directory: &Path) -> Result<SessionDiscovery, IpcError> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(SessionDiscovery {
            live: Vec::new(),
            unreachable: Vec::new(),
        });
    };
    let mut live = Vec::new();
    let mut unreachable = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(descriptor) = serde_json::from_slice::<SessionDescriptor>(&bytes) else {
            let _ = fs::remove_file(path);
            continue;
        };
        if descriptor.protocol_version != SESSION_PROTOCOL_VERSION {
            continue;
        }
        if descriptor_process_is_dead(&descriptor) {
            let _ = fs::remove_file(path);
            continue;
        }
        match probe_descriptor(&descriptor).await {
            Ok(()) => live.push(descriptor),
            Err(reason) => unreachable.push(UnreachableSession {
                session_id: descriptor.session_id,
                descriptor_path: path,
                reason,
            }),
        }
    }
    live.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    unreachable.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    Ok(SessionDiscovery { live, unreachable })
}

#[cfg(target_os = "linux")]
fn descriptor_process_is_dead(descriptor: &SessionDescriptor) -> bool {
    !Path::new("/proc").join(descriptor.pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
const fn descriptor_process_is_dead(_descriptor: &SessionDescriptor) -> bool {
    false
}

async fn probe_descriptor(descriptor: &SessionDescriptor) -> Result<(), String> {
    let address = std::net::SocketAddr::V4(descriptor.address);
    timeout(CONNECT_TIMEOUT, async {
        let mut stream = TcpStream::connect(address).await?;
        write_frame(
            &mut stream,
            &RequestEnvelope {
                protocol_version: SESSION_PROTOCOL_VERSION,
                token: descriptor.token.clone(),
                request: SessionRequest::Ping,
            },
        )
        .await?;
        let response: ResponseEnvelope = read_frame(&mut stream).await?;
        match response.result {
            Ok(SessionResponse::Pong { .. })
                if response.protocol_version == SESSION_PROTOCOL_VERSION =>
            {
                Ok(())
            }
            Err(error) => Err(IpcError::Remote(error)),
            _ => Err(IpcError::ProtocolMismatch(response.protocol_version)),
        }
    })
    .await
    .map_err(|_| "authenticated endpoint probe timed out".to_owned())?
    .map_err(|error| error.to_string())
}

fn select_descriptor(
    mut discovery: SessionDiscovery,
    explicit: Option<&str>,
) -> Result<SessionDescriptor, IpcError> {
    let selector = explicit
        .map(str::to_owned)
        .or_else(|| env::var(SESSION_SELECTOR_ENV).ok());
    if let Some(selector) = selector {
        if let Some(descriptor) = discovery
            .live
            .into_iter()
            .find(|descriptor| descriptor.session_id == selector)
        {
            return Ok(descriptor);
        }
        if let Some(session) = discovery
            .unreachable
            .into_iter()
            .find(|session| session.session_id == selector)
        {
            return Err(IpcError::SelectedSessionUnreachable(session));
        }
        return Err(IpcError::SelectedSessionNotFound(selector));
    }
    match discovery.live.len() {
        0 if discovery.unreachable.is_empty() => Err(IpcError::ApplicationNotRunning),
        0 => Err(IpcError::SessionsUnreachable(discovery.unreachable)),
        1 => Ok(discovery.live.remove(0)),
        _ => Err(IpcError::AmbiguousSessions(
            discovery
                .live
                .into_iter()
                .map(|item| item.session_id)
                .collect(),
        )),
    }
}

fn session_directory() -> Result<PathBuf, IpcError> {
    if let Some(path) = env::var_os(RUNTIME_DIRECTORY_ENV) {
        return Ok(PathBuf::from(path));
    }
    if let Some(base) = BaseDirs::new().and_then(|dirs| dirs.runtime_dir().map(Path::to_path_buf)) {
        return Ok(base.join("perfboard-designer"));
    }
    ProjectDirs::from("dev", "perfboarddesigner", "Perfboard Designer")
        .map(|dirs| dirs.data_local_dir().join("runtime"))
        .ok_or(IpcError::RuntimeDirectoryUnavailable)
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Tells the client its response did not fit in a frame, rather than closing
/// the connection with no explanation.
fn too_large_response(size: usize) -> ResponseEnvelope {
    ResponseEnvelope {
        protocol_version: SESSION_PROTOCOL_VERSION,
        result: Err(SessionApiError::new(
            "session.response_too_large",
            format!(
                "the response is {size} bytes, over the {MAX_FRAME_BYTES}-byte session frame \
                 limit; request less data"
            ),
        )),
    }
}

async fn read_frame<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> Result<T, IpcError> {
    let size = stream.read_u32().await? as usize;
    if size > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge(size));
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn write_frame<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<(), IpcError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge(bytes.len()));
    }
    let size = u32::try_from(bytes.len()).map_err(|_| IpcError::FrameTooLarge(bytes.len()))?;
    stream.write_u32(size).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("Perfboard Designer is not running")]
    ApplicationNotRunning,
    #[error(
        "multiple Perfboard Designer sessions are running; set {SESSION_SELECTOR_ENV} to one of {0:?}"
    )]
    AmbiguousSessions(Vec<String>),
    #[error("selected Perfboard Designer session `{0}` was not found")]
    SelectedSessionNotFound(String),
    #[error(
        "selected Perfboard Designer session `{0:?}` was discovered but its authenticated endpoint is unreachable"
    )]
    SelectedSessionUnreachable(UnreachableSession),
    #[error(
        "Perfboard Designer session descriptors were found, but their authenticated endpoints are unreachable: {0:?}"
    )]
    SessionsUnreachable(Vec<UnreachableSession>),
    #[error("the per-user runtime directory is unavailable")]
    RuntimeDirectoryUnavailable,
    #[error("session connection timed out")]
    ConnectionTimeout,
    #[error("session request timed out")]
    RequestTimeout,
    #[error("session returned protocol version {0}")]
    ProtocolMismatch(u32),
    #[error("session frame is too large ({0} bytes)")]
    FrameTooLarge(usize),
    #[error("unexpected IPv6 listener")]
    UnexpectedIpv6,
    #[error("session request failed: {0:?}")]
    Remote(SessionApiError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PingHandler;

    impl SessionRequestHandler for PingHandler {
        fn handle(&self, request: SessionRequest) -> Result<SessionResponse, SessionApiError> {
            match request {
                SessionRequest::Ping => Ok(SessionResponse::Pong {
                    revision: DocumentRevision(7),
                    project_name: "IPC fixture".into(),
                }),
                _ => Err(SessionApiError::new(
                    "test.unexpected",
                    "unexpected request",
                )),
            }
        }
    }

    #[test]
    fn route_alternatives_are_optional_on_the_wire() {
        // A request from an adapter that predates alternatives still parses,
        // and asks for none.
        let old = serde_json::json!({
            "method": "suggest_routes",
            "params": { "requests": [] }
        });
        let request = serde_json::from_value::<SessionRequest>(old.clone()).unwrap();
        assert_eq!(
            request,
            SessionRequest::SuggestRoutes {
                requests: Vec::new(),
                alternatives: false
            }
        );
        assert_eq!(serde_json::to_value(&request).unwrap(), old);

        let results = RouteResults {
            revision: DocumentRevision(2),
            proposals: Vec::new(),
            alternatives: Vec::new(),
        };
        let json = serde_json::to_value(&results).unwrap();
        assert_eq!(json, serde_json::json!({ "revision": 2, "proposals": [] }));
        assert_eq!(
            serde_json::from_value::<RouteResults>(json).unwrap(),
            results
        );
    }

    #[test]
    fn review_note_requests_use_a_stable_wire_shape() {
        let json = serde_json::json!({
            "method": "add_review_notes",
            "params": { "notes": [
                { "severity": "critical", "title": "A/B swapped" },
                { "severity": "info", "title": "Tidy", "detail": "Nice",
                  "objects": [{ "kind": "net", "id": "n1" }] }
            ] }
        });
        let request = serde_json::from_value::<SessionRequest>(json).unwrap();
        let SessionRequest::AddReviewNotes { notes } = &request else {
            panic!("unexpected request {request:?}");
        };
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].detail, "");
        assert_eq!(notes[1].objects[0].kind, "net");

        let id = ReviewNoteId::new();
        let json =
            serde_json::to_value(SessionRequest::RemoveReviewNotes { note_ids: vec![id] }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "method": "remove_review_notes",
                "params": { "note_ids": [id.to_string()] }
            })
        );

        let json = serde_json::json!({
            "method": "move_review_note",
            "params": { "note_id": id.to_string() }
        });
        assert_eq!(
            serde_json::from_value::<SessionRequest>(json).unwrap(),
            SessionRequest::MoveReviewNote {
                note_id: id,
                before: None
            }
        );

        let response = SessionResponse::ReviewNotes(ReviewNotesChanged {
            revision: DocumentRevision(5),
            notes: Vec::new(),
            summary: "Removed 0 review notes".into(),
        });
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["type"], "review_notes");
        assert_eq!(
            serde_json::from_value::<SessionResponse>(json).unwrap(),
            response
        );
    }

    #[test]
    fn rename_project_uses_a_stable_wire_shape() {
        let request = SessionRequest::RenameProject {
            expected_revision: DocumentRevision(3),
            name: "Fuzz pedal".into(),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "method": "rename_project",
                "params": { "expected_revision": 3, "name": "Fuzz pedal" }
            })
        );
        assert_eq!(
            serde_json::from_value::<SessionRequest>(json).unwrap(),
            request
        );

        let response = SessionResponse::Renamed(ProjectRenamed {
            revision: DocumentRevision(4),
            name: "Fuzz pedal".into(),
            summary: "Renamed project Untitled project to Fuzz pedal".into(),
        });
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["type"], "renamed");
        assert_eq!(json["value"]["revision"], 4);
        assert_eq!(
            serde_json::from_value::<SessionResponse>(json).unwrap(),
            response
        );
    }

    struct OversizedHandler;

    impl SessionRequestHandler for OversizedHandler {
        fn handle(&self, request: SessionRequest) -> Result<SessionResponse, SessionApiError> {
            if request == SessionRequest::Ping {
                return PingHandler.handle(request);
            }
            Ok(SessionResponse::Rendered(RenderedDesign {
                revision: DocumentRevision(1),
                side: Side::Component,
                mime_type: "image/svg+xml".into(),
                svg: "x".repeat(MAX_FRAME_BYTES + 1),
            }))
        }
    }

    #[tokio::test]
    async fn oversized_response_is_reported_instead_of_dropped() {
        let directory = temporary_directory();
        let (registration, listener) = SessionRegistration::bind_in(&directory).unwrap();
        let task = tokio::spawn(serve(
            listener,
            Arc::clone(&registration),
            Arc::new(OversizedHandler),
        ));
        let error = SessionClient::with_runtime_directory(directory.clone())
            .call(SessionRequest::GetProject)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, IpcError::Remote(remote) if remote.code == "session.response_too_large"),
            "{error:?}"
        );
        task.abort();
        drop(registration);
        fs::remove_dir_all(directory).unwrap();
    }

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!("perfboard-ipc-test-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn client_discovers_and_authenticates_with_host() {
        let directory = temporary_directory();
        let (registration, listener) = SessionRegistration::bind_in(&directory).unwrap();
        let task = tokio::spawn(serve(
            listener,
            Arc::clone(&registration),
            Arc::new(PingHandler),
        ));
        let response = SessionClient::with_runtime_directory(directory.clone())
            .call(SessionRequest::Ping)
            .await
            .unwrap();
        assert_eq!(
            response,
            SessionResponse::Pong {
                revision: DocumentRevision(7),
                project_name: "IPC fixture".into(),
            }
        );
        task.abort();
        drop(registration);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn host_rejects_an_invalid_session_token() {
        let directory = temporary_directory();
        let (registration, listener) = SessionRegistration::bind_in(&directory).unwrap();
        let address = registration.descriptor.address;
        let task = tokio::spawn(serve(
            listener,
            Arc::clone(&registration),
            Arc::new(PingHandler),
        ));
        let mut stream = TcpStream::connect(address).await.unwrap();
        write_frame(
            &mut stream,
            &RequestEnvelope {
                protocol_version: SESSION_PROTOCOL_VERSION,
                token: "not-the-session-token".into(),
                request: SessionRequest::Ping,
            },
        )
        .await
        .unwrap();
        let response: ResponseEnvelope = read_frame(&mut stream).await.unwrap();
        assert!(matches!(
            response.result,
            Err(SessionApiError { ref code, .. }) if code == "session.unauthorized"
        ));
        task.abort();
        drop(registration);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn multiple_sessions_require_an_explicit_selector() {
        let directory = temporary_directory();
        let (first, first_listener) = SessionRegistration::bind_in(&directory).unwrap();
        let (second, second_listener) = SessionRegistration::bind_in(&directory).unwrap();
        let first_task = tokio::spawn(serve(
            first_listener,
            Arc::clone(&first),
            Arc::new(PingHandler),
        ));
        let second_task = tokio::spawn(serve(
            second_listener,
            Arc::clone(&second),
            Arc::new(PingHandler),
        ));
        let error = SessionClient::with_runtime_directory(directory.clone())
            .call(SessionRequest::Ping)
            .await
            .unwrap_err();
        assert!(matches!(error, IpcError::AmbiguousSessions(_)));
        let response = SessionClient::with_runtime_directory(directory.clone())
            .with_session_id(first.session_id())
            .call(SessionRequest::Ping)
            .await
            .unwrap();
        assert!(matches!(response, SessionResponse::Pong { .. }));
        first_task.abort();
        second_task.abort();
        drop(first);
        drop(second);
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn unreachable_descriptor_is_reported_and_preserved() {
        let directory = temporary_directory();
        let (registration, listener) = SessionRegistration::bind_in(&directory).unwrap();
        let descriptor_path = registration.descriptor_path.clone();
        drop(listener);

        let error = SessionClient::with_runtime_directory(directory.clone())
            .call(SessionRequest::Ping)
            .await
            .unwrap_err();

        assert!(
            matches!(error, IpcError::SessionsUnreachable(ref sessions) if
            sessions.len() == 1 && sessions[0].session_id == registration.session_id())
        );
        assert!(descriptor_path.exists());
        drop(registration);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn descriptor_owned_by_a_dead_linux_process_is_removed() {
        let directory = temporary_directory();
        let (registration, listener) = SessionRegistration::bind_in(&directory).unwrap();
        let descriptor_path = registration.descriptor_path.clone();
        let mut stale = registration.descriptor.clone();
        stale.pid = u32::MAX;
        fs::write(&descriptor_path, serde_json::to_vec(&stale).unwrap()).unwrap();
        drop(listener);

        let error = SessionClient::with_runtime_directory(directory.clone())
            .call(SessionRequest::Ping)
            .await
            .unwrap_err();

        assert!(matches!(error, IpcError::ApplicationNotRunning));
        assert!(!descriptor_path.exists());
        drop(registration);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn descriptor_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let directory = temporary_directory();
        let (registration, _listener) = SessionRegistration::bind_in(&directory).unwrap();
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&registration.descriptor_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(registration);
        fs::remove_dir_all(directory).unwrap();
    }
}
