use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use perfboard_core::{ComponentDefinition, ComponentDefinitionError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFINITION_FILE: &str = "component.toml";
const DOCUMENTATION_FILE: &str = "README.md";
const WATCH_DEBOUNCE: Duration = Duration::from_millis(250);
const WATCH_POLL: Duration = Duration::from_millis(100);

pub const COMPONENT_VIEWER_EVENT: &str = "component-viewer-changed";

#[derive(Clone, Debug)]
pub struct AuthorizedComponent {
    root: PathBuf,
    directory: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentWatcherStatus {
    Watching,
    Error,
    Stopped,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComponentViewerSnapshot {
    pub generation: u64,
    pub watcher_status: ComponentWatcherStatus,
    pub attempted_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<RuntimeComponentLoad>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watcher_error: Option<String>,
}

pub struct ComponentWatch {
    _watcher: RecommendedWatcher,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ComponentLoadDiagnostic {
    pub code: &'static str,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeComponentLoad {
    pub definition: Option<ComponentDefinition>,
    pub documentation: Option<String>,
    pub component_path: String,
    pub revision: String,
    pub diagnostics: Vec<ComponentLoadDiagnostic>,
}

#[derive(Debug, Error)]
pub enum ComponentSaveError {
    #[error("component changed on disk (expected {expected}, found {actual})")]
    Conflict { expected: String, actual: String },
    #[error("save-as target already contains component.toml")]
    TargetExists,
    #[error("draft is invalid: {0}")]
    InvalidDraft(String),
    #[error("could not save component: {0}")]
    Io(#[from] std::io::Error),
}

impl AuthorizedComponent {
    pub fn new(root: &Path, directory: &Path) -> Result<Self, ComponentLoadDiagnostic> {
        let root = canonical_directory(root, "component.root_unavailable")?;
        let directory = canonical_directory(directory, "component.directory_unavailable")?;
        if !directory.starts_with(&root) {
            return Err(diagnostic(
                "component.outside_authorized_root",
                "selected component directory is outside the authorized library root",
                directory.display().to_string(),
            ));
        }
        Ok(Self { root, directory })
    }

    pub fn load(&self) -> RuntimeComponentLoad {
        load_component(&self.root, &self.directory)
    }

    pub fn save(
        &self,
        definition_toml: &str,
        documentation: &str,
        expected_revision: Option<&str>,
    ) -> Result<RuntimeComponentLoad, ComponentSaveError> {
        let current = self.load();
        match expected_revision {
            Some(expected) if current.revision != expected => {
                return Err(ComponentSaveError::Conflict {
                    expected: expected.to_owned(),
                    actual: current.revision,
                });
            }
            None if self.directory.join(DEFINITION_FILE).exists() => {
                return Err(ComponentSaveError::TargetExists);
            }
            _ => {}
        }
        ComponentDefinition::from_toml(definition_toml)
            .map_err(|error| ComponentSaveError::InvalidDraft(error.to_string()))?;
        replace_file_safely(
            &self.directory,
            DEFINITION_FILE,
            definition_toml.as_bytes(),
            true,
        )?;
        replace_file_safely(
            &self.directory,
            DOCUMENTATION_FILE,
            documentation.as_bytes(),
            false,
        )?;
        let loaded = self.load();
        if loaded.definition.is_none() {
            return Err(ComponentSaveError::InvalidDraft(
                loaded.diagnostics.first().map_or_else(
                    || "saved component did not reload".into(),
                    |item| item.message.clone(),
                ),
            ));
        }
        Ok(loaded)
    }
}

fn replace_file_safely(
    directory: &Path,
    name: &str,
    contents: &[u8],
    validate_component: bool,
) -> Result<(), ComponentSaveError> {
    let target = directory.join(name);
    if target.exists() {
        let canonical = fs::canonicalize(&target)?;
        if !canonical.starts_with(directory) || !canonical.is_file() {
            return Err(ComponentSaveError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{name} resolves outside the component directory"),
            )));
        }
        fs::copy(&canonical, directory.join(format!("{name}.bak")))?;
    }
    let temporary = directory.join(format!(".{name}.ce3.tmp"));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if validate_component {
            let staged = fs::read_to_string(&temporary)?;
            ComponentDefinition::from_toml(&staged)
                .map_err(|error| ComponentSaveError::InvalidDraft(error.to_string()))?;
        }
        fs::rename(&temporary, &target)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl ComponentViewerSnapshot {
    pub fn watching(generation: u64, load: RuntimeComponentLoad) -> Self {
        Self {
            generation,
            watcher_status: ComponentWatcherStatus::Watching,
            attempted_at_ms: unix_time_ms(),
            load: Some(load),
            watcher_error: None,
        }
    }

    fn watcher_error(generation: u64, message: String) -> Self {
        Self {
            generation,
            watcher_status: ComponentWatcherStatus::Error,
            attempted_at_ms: unix_time_ms(),
            load: None,
            watcher_error: Some(message),
        }
    }

    pub fn watcher_unavailable(
        generation: u64,
        load: RuntimeComponentLoad,
        message: String,
    ) -> Self {
        Self {
            generation,
            watcher_status: ComponentWatcherStatus::Error,
            attempted_at_ms: unix_time_ms(),
            load: Some(load),
            watcher_error: Some(message),
        }
    }

    pub fn stopped(generation: u64) -> Self {
        Self {
            generation,
            watcher_status: ComponentWatcherStatus::Stopped,
            attempted_at_ms: unix_time_ms(),
            load: None,
            watcher_error: None,
        }
    }
}

impl ComponentWatch {
    pub fn start(
        component: AuthorizedComponent,
        generation: u64,
        active_generation: Arc<AtomicU64>,
        publish: impl Fn(ComponentViewerSnapshot) + Send + 'static,
    ) -> Result<Self, notify::Error> {
        let (events, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = events.send(event);
        })?;
        watcher.watch(&component.directory, RecursiveMode::NonRecursive)?;

        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            watch_worker(
                &component,
                generation,
                &active_generation,
                &worker_stop,
                &receiver,
                &publish,
            );
        });
        Ok(Self {
            _watcher: watcher,
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for ComponentWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn watch_worker(
    component: &AuthorizedComponent,
    generation: u64,
    active_generation: &AtomicU64,
    stop: &AtomicBool,
    receiver: &mpsc::Receiver<notify::Result<Event>>,
    publish: &impl Fn(ComponentViewerSnapshot),
) {
    while !stop.load(Ordering::Acquire) {
        let first = match receiver.recv_timeout(WATCH_POLL) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut should_reload = false;
        let mut error = None;
        record_watch_event(first, &component.directory, &mut should_reload, &mut error);
        let mut deadline = Instant::now() + WATCH_DEBOUNCE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(event) => {
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    record_watch_event(event, &component.directory, &mut should_reload, &mut error);
                    deadline = Instant::now() + WATCH_DEBOUNCE;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        if stop.load(Ordering::Acquire) || active_generation.load(Ordering::Acquire) != generation {
            continue;
        }
        if let Some(message) = error {
            publish(ComponentViewerSnapshot::watcher_error(generation, message));
        }
        if should_reload {
            let load = component.load();
            if !stop.load(Ordering::Acquire)
                && active_generation.load(Ordering::Acquire) == generation
            {
                publish(ComponentViewerSnapshot::watching(generation, load));
            }
        }
    }
}

fn record_watch_event(
    event: notify::Result<Event>,
    directory: &Path,
    should_reload: &mut bool,
    error: &mut Option<String>,
) {
    match event {
        Ok(event) if is_component_file_event(&event, directory) => *should_reload = true,
        Ok(_) => {}
        Err(watch_error) => *error = Some(watch_error.to_string()),
    }
}

fn is_component_file_event(event: &Event, directory: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        path == directory
            || path
                .file_name()
                .is_some_and(|name| name == DEFINITION_FILE || name == DOCUMENTATION_FILE)
    })
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn canonical_directory(
    path: &Path,
    code: &'static str,
) -> Result<PathBuf, ComponentLoadDiagnostic> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        diagnostic(
            code,
            format!("could not access directory: {error}"),
            path.display().to_string(),
        )
    })?;
    if !canonical.is_dir() {
        return Err(diagnostic(
            code,
            "selected path is not a directory",
            canonical.display().to_string(),
        ));
    }
    Ok(canonical)
}

fn load_component(root: &Path, directory: &Path) -> RuntimeComponentLoad {
    let component_path = directory.display().to_string();
    let definition_path = directory.join(DEFINITION_FILE);
    let documentation_path = directory.join(DOCUMENTATION_FILE);
    let mut hasher = Sha256::new();

    let definition_source = match read_scoped_file(root, &definition_path, true) {
        Ok(Some(bytes)) => {
            hash_file(&mut hasher, DEFINITION_FILE, Some(&bytes));
            match String::from_utf8(bytes) {
                Ok(source) => source,
                Err(error) => {
                    return failed_load(
                        component_path,
                        hasher,
                        diagnostic(
                            "component.definition_not_utf8",
                            format!("component.toml is not valid UTF-8: {error}"),
                            DEFINITION_FILE,
                        ),
                    );
                }
            }
        }
        Ok(None) => unreachable!("required files never return None"),
        Err(mut error) => {
            hash_file(&mut hasher, DEFINITION_FILE, None);
            if error.code == "component.definition_missing" {
                error.message = missing_definition_message(directory);
            }
            return failed_load(component_path, hasher, error);
        }
    };

    let documentation = match read_scoped_file(root, &documentation_path, false) {
        Ok(Some(bytes)) => {
            hash_file(&mut hasher, DOCUMENTATION_FILE, Some(&bytes));
            match String::from_utf8(bytes) {
                Ok(markdown) => Some(markdown),
                Err(error) => {
                    return failed_load(
                        component_path,
                        hasher,
                        diagnostic(
                            "component.documentation_not_utf8",
                            format!("README.md is not valid UTF-8: {error}"),
                            DOCUMENTATION_FILE,
                        ),
                    );
                }
            }
        }
        Ok(None) => {
            hash_file(&mut hasher, DOCUMENTATION_FILE, None);
            None
        }
        Err(error) => return failed_load(component_path, hasher, error),
    };

    let definition: ComponentDefinition = match toml::from_str(&definition_source) {
        Ok(definition) => definition,
        Err(error) => {
            let (line, column) = error
                .span()
                .map(|span| line_column(&definition_source, span.start))
                .map_or((None, None), |(line, column)| (Some(line), Some(column)));
            return RuntimeComponentLoad {
                definition: None,
                documentation,
                component_path,
                revision: finish_hash(hasher),
                diagnostics: vec![ComponentLoadDiagnostic {
                    code: "component.invalid_toml",
                    severity: DiagnosticSeverity::Error,
                    message: error.message().to_owned(),
                    file: DEFINITION_FILE.into(),
                    line,
                    column,
                    field: None,
                }],
            };
        }
    };

    if let Err(error) = definition.validate() {
        return RuntimeComponentLoad {
            definition: None,
            documentation,
            component_path,
            revision: finish_hash(hasher),
            diagnostics: vec![validation_diagnostic(&error)],
        };
    }

    RuntimeComponentLoad {
        definition: Some(definition),
        documentation,
        component_path,
        revision: finish_hash(hasher),
        diagnostics: Vec::new(),
    }
}

fn read_scoped_file(
    root: &Path,
    path: &Path,
    required: bool,
) -> Result<Option<Vec<u8>>, ComponentLoadDiagnostic> {
    let canonical = match fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(diagnostic(
                if error.kind() == std::io::ErrorKind::NotFound {
                    "component.definition_missing"
                } else {
                    "component.file_unavailable"
                },
                format!("could not read {}: {error}", file_name(path)),
                file_name(path),
            ));
        }
    };
    if !canonical.starts_with(root) {
        return Err(diagnostic(
            "component.symlink_escape",
            format!(
                "{} resolves outside the authorized library root",
                file_name(path)
            ),
            file_name(path),
        ));
    }
    if !canonical.is_file() {
        return Err(diagnostic(
            "component.file_unavailable",
            format!("{} is not a regular file", file_name(path)),
            file_name(path),
        ));
    }
    fs::read(&canonical).map(Some).map_err(|error| {
        diagnostic(
            "component.file_unavailable",
            format!("could not read {}: {error}", file_name(path)),
            file_name(path),
        )
    })
}

fn validation_diagnostic(error: &ComponentDefinitionError) -> ComponentLoadDiagnostic {
    let (code, field) = match &error {
        ComponentDefinitionError::UnsupportedSchema(_) => (
            "component.unsupported_schema",
            Some("schema_version".into()),
        ),
        ComponentDefinitionError::BlankField { field } => {
            ("component.blank_field", Some((*field).into()))
        }
        ComponentDefinitionError::InvalidBody => ("component.invalid_body", Some("body".into())),
        ComponentDefinitionError::InvalidRotations => (
            "component.invalid_rotations",
            Some("allowed_rotations".into()),
        ),
        ComponentDefinitionError::DuplicatePinId(id) => {
            ("component.duplicate_pin_id", Some(format!("pins.{id}")))
        }
        ComponentDefinitionError::DuplicatePinNumber(number) => (
            "component.duplicate_pin_number",
            Some(format!("pins.{number}")),
        ),
        ComponentDefinitionError::BlankPinNumber(id) => (
            "component.blank_pin_number",
            Some(format!("pins.{id}.number")),
        ),
        ComponentDefinitionError::BlankPinName(id) => {
            ("component.blank_pin_name", Some(format!("pins.{id}.name")))
        }
        ComponentDefinitionError::InvalidRectangle { kind, label } => (
            "component.invalid_rectangle",
            Some(format!("{kind}.{label}")),
        ),
        ComponentDefinitionError::InvalidBridgePins => {
            ("component.invalid_bridge_pins", Some("pins".into()))
        }
        ComponentDefinitionError::InvalidBridgeExclusions => {
            ("component.invalid_bridge_exclusions", Some("kind".into()))
        }
        ComponentDefinitionError::InvalidJumperPins => {
            ("component.invalid_jumper_pins", Some("pins".into()))
        }
        ComponentDefinitionError::InvalidToml(_) => ("component.invalid_toml", None),
        ComponentDefinitionError::DuplicateDefinitionId(_) => {
            ("component.duplicate_definition_id", Some("id".into()))
        }
    };
    ComponentLoadDiagnostic {
        code,
        severity: DiagnosticSeverity::Error,
        message: error.to_string(),
        file: DEFINITION_FILE.into(),
        line: None,
        column: None,
        field,
    }
}

/// Explains a folder without `component.toml`, naming the parts inside it
/// when it is a library folder (one part per subfolder) rather than a part.
fn missing_definition_message(directory: &Path) -> String {
    let mut parts = fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join(DEFINITION_FILE).is_file())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if parts.is_empty() {
        return "This folder has no component.toml. Open the folder of a single part.".into();
    }
    parts.sort();
    let shown = parts.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
    let more = match parts.len() {
        0..=4 => String::new(),
        count => format!(" and {} more", count - 4),
    };
    format!(
        "This is a component library folder, not a part. Open one of the part folders inside it: {shown}{more}."
    )
}

fn diagnostic(
    code: &'static str,
    message: impl Into<String>,
    file: impl Into<String>,
) -> ComponentLoadDiagnostic {
    ComponentLoadDiagnostic {
        code,
        severity: DiagnosticSeverity::Error,
        message: message.into(),
        file: file.into(),
        line: None,
        column: None,
        field: None,
    }
}

fn failed_load(
    component_path: String,
    hasher: Sha256,
    diagnostic: ComponentLoadDiagnostic,
) -> RuntimeComponentLoad {
    RuntimeComponentLoad {
        definition: None,
        documentation: None,
        component_path,
        revision: finish_hash(hasher),
        diagnostics: vec![diagnostic],
    }
}

fn hash_file(hasher: &mut Sha256, name: &str, contents: Option<&[u8]>) {
    hasher.update(name.as_bytes());
    hasher.update([0]);
    match contents {
        Some(contents) => {
            hasher.update([1]);
            hasher.update((contents.len() as u64).to_le_bytes());
            hasher.update(contents);
        }
        None => hasher.update([0]),
    }
}

fn finish_hash(hasher: Sha256) -> String {
    format!("sha256:{:x}", hasher.finalize())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn line_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source[..byte_offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    const VALID: &str = r#"
schema_version = 1
id = "dev.example.test"
definition_version = "1.0.0"
name = "Test"
allowed_rotations = ["deg0"]
pins = []
[body]
x = 0.0
y = 0.0
width = 2.54
height = 2.54
"#;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "perfboard-component-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_component(directory: &Path, source: &str) {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join(DEFINITION_FILE), source).unwrap();
    }

    fn receive_load(receiver: &mpsc::Receiver<ComponentViewerSnapshot>) -> RuntimeComponentLoad {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("watcher did not publish a load before the timeout");
            if let Some(load) = snapshot.load {
                return load;
            }
        }
    }

    #[test]
    fn a_library_folder_names_the_parts_inside_it() {
        let root = TestDir::new("library-folder");
        for name in ["b-part", "a-part", "c", "d", "e"] {
            write_component(&root.0.join(name), VALID);
        }
        fs::create_dir_all(root.0.join("not-a-part")).unwrap();
        let load = AuthorizedComponent::new(&root.0, &root.0).unwrap().load();
        assert_eq!(load.diagnostics[0].code, "component.definition_missing");
        assert_eq!(
            load.diagnostics[0].message,
            "This is a component library folder, not a part. Open one of the part folders \
             inside it: a-part, b-part, c, d and 1 more."
        );

        let empty = TestDir::new("empty-folder");
        let load = AuthorizedComponent::new(&empty.0, &empty.0).unwrap().load();
        assert_eq!(load.diagnostics[0].code, "component.definition_missing");
        assert!(
            load.diagnostics[0]
                .message
                .starts_with("This folder has no component.toml")
        );
    }

    #[test]
    fn loads_valid_definition_with_optional_documentation_and_stable_revision() {
        let root = TestDir::new("valid");
        write_component(&root.0, VALID);
        fs::write(root.0.join(DOCUMENTATION_FILE), "# Notes").unwrap();
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        let first = authorized.load();
        let second = authorized.load();
        assert_eq!(first.definition.unwrap().id, "dev.example.test");
        assert_eq!(first.documentation.as_deref(), Some("# Notes"));
        assert_eq!(first.diagnostics, Vec::new());
        assert_eq!(first.revision, second.revision);
    }

    #[test]
    fn missing_readme_is_valid_and_content_changes_update_revision() {
        let root = TestDir::new("reload");
        write_component(&root.0, VALID);
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        let first = authorized.load();
        assert!(first.documentation.is_none());
        fs::write(root.0.join(DOCUMENTATION_FILE), "new docs").unwrap();
        let second = authorized.load();
        assert_ne!(first.revision, second.revision);
    }

    #[test]
    fn reports_missing_malformed_and_invalid_definitions() {
        let root = TestDir::new("errors");
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        assert_eq!(
            authorized.load().diagnostics[0].code,
            "component.definition_missing"
        );
        write_component(&root.0, "not = [valid");
        let malformed = authorized.load();
        assert_eq!(malformed.diagnostics[0].code, "component.invalid_toml");
        assert!(malformed.diagnostics[0].line.is_some());
        write_component(&root.0, &VALID.replace("width = 2.54", "width = 0.0"));
        let invalid = authorized.load();
        assert_eq!(invalid.diagnostics[0].code, "component.invalid_body");
        assert_eq!(invalid.diagnostics[0].field.as_deref(), Some("body"));
    }

    #[test]
    fn rejects_directories_outside_the_authorized_root() {
        let root = TestDir::new("root");
        let outside = TestDir::new("outside");
        let error = AuthorizedComponent::new(&root.0, &outside.0).unwrap_err();
        assert_eq!(error.code, "component.outside_authorized_root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_component_and_readme_symlink_escapes() {
        use std::os::unix::fs::symlink;
        let root = TestDir::new("symlink-root");
        let outside = TestDir::new("symlink-outside");
        fs::write(outside.0.join(DEFINITION_FILE), VALID).unwrap();
        symlink(
            outside.0.join(DEFINITION_FILE),
            root.0.join(DEFINITION_FILE),
        )
        .unwrap();
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        assert_eq!(
            authorized.load().diagnostics[0].code,
            "component.symlink_escape"
        );

        fs::remove_file(root.0.join(DEFINITION_FILE)).unwrap();
        fs::write(root.0.join(DEFINITION_FILE), VALID).unwrap();
        fs::write(outside.0.join(DOCUMENTATION_FILE), "outside").unwrap();
        symlink(
            outside.0.join(DOCUMENTATION_FILE),
            root.0.join(DOCUMENTATION_FILE),
        )
        .unwrap();
        assert_eq!(
            authorized.load().diagnostics[0].code,
            "component.symlink_escape"
        );
    }

    #[test]
    fn allows_a_component_beneath_an_authorized_library_root() {
        let root = TestDir::new("nested");
        let component = root.0.join("parts").join("example");
        write_component(&component, VALID);
        let load = AuthorizedComponent::new(&root.0, &component)
            .unwrap()
            .load();
        assert!(load.definition.is_some());
    }

    #[test]
    fn save_is_atomic_round_trips_keeps_backup_and_detects_conflicts() {
        let root = TestDir::new("save");
        write_component(&root.0, VALID);
        fs::write(root.0.join(DOCUMENTATION_FILE), "old docs").unwrap();
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        let revision = authorized.load().revision;
        let changed = VALID.replace("name = \"Test\"", "name = \"Changed\"");
        let saved = authorized
            .save(&changed, "new docs", Some(&revision))
            .unwrap();
        assert_eq!(saved.definition.unwrap().name, "Changed");
        assert_eq!(saved.documentation.as_deref(), Some("new docs"));
        assert_eq!(
            fs::read_to_string(root.0.join("component.toml.bak")).unwrap(),
            VALID
        );
        assert_eq!(
            fs::read_to_string(root.0.join("README.md.bak")).unwrap(),
            "old docs"
        );
        let error = authorized.save(VALID, "", Some(&revision)).unwrap_err();
        assert!(matches!(error, ComponentSaveError::Conflict { .. }));
        assert_eq!(authorized.load().definition.unwrap().name, "Changed");
    }

    #[test]
    fn failed_save_does_not_replace_the_definition() {
        let root = TestDir::new("failed-save");
        write_component(&root.0, VALID);
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        let revision = authorized.load().revision;
        assert!(authorized.save("not = [toml", "", Some(&revision)).is_err());
        assert_eq!(
            fs::read_to_string(root.0.join(DEFINITION_FILE)).unwrap(),
            VALID
        );
    }

    #[test]
    fn watcher_debounces_edits_recovers_and_ignores_an_old_generation() {
        let root = TestDir::new("watch");
        write_component(&root.0, VALID);
        let authorized = AuthorizedComponent::new(&root.0, &root.0).unwrap();
        let generation = Arc::new(AtomicU64::new(7));
        let (sender, receiver) = mpsc::channel();
        let watch =
            ComponentWatch::start(authorized, 7, Arc::clone(&generation), move |snapshot| {
                sender.send(snapshot).unwrap();
            })
            .unwrap();

        for source in ["not = [valid", "still = [invalid", "broken = [toml"] {
            write_component(&root.0, source);
        }
        let invalid = receive_load(&receiver);
        assert!(invalid.definition.is_none());
        assert_eq!(invalid.diagnostics[0].code, "component.invalid_toml");
        assert!(receiver.recv_timeout(Duration::from_millis(500)).is_err());

        let replacement = root.0.join("component.toml.next");
        fs::write(
            &replacement,
            VALID.replace("name = \"Test\"", "name = \"Recovered\""),
        )
        .unwrap();
        fs::rename(replacement, root.0.join(DEFINITION_FILE)).unwrap();
        let recovered = receive_load(&receiver);
        assert_eq!(recovered.definition.unwrap().name, "Recovered");

        fs::remove_file(root.0.join(DEFINITION_FILE)).unwrap();
        assert_eq!(
            receive_load(&receiver).diagnostics[0].code,
            "component.definition_missing"
        );
        write_component(&root.0, VALID);
        assert!(receive_load(&receiver).definition.is_some());

        generation.store(8, Ordering::Release);
        write_component(&root.0, "late = [invalid");
        assert!(receiver.recv_timeout(Duration::from_millis(700)).is_err());
        drop(watch);
    }
}
