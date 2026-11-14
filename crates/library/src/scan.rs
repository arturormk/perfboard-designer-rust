//! Reading one layer folder: one component per subfolder, every file confined
//! to the layer's root.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use perfboard_core::{ComponentDefinition, DiagnosticSeverity};

pub const DEFINITION_FILE: &str = "component.toml";
pub const DOCUMENTATION_FILE: &str = "README.md";

pub struct LoadedComponent {
    pub definition: ComponentDefinition,
    pub documentation: Option<String>,
    pub directory: Option<PathBuf>,
    /// The folder name, for ordering and diagnostics.
    pub name: String,
}

pub struct ScanProblem {
    pub code: &'static str,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub path: PathBuf,
}

/// Loads every `<layer>/<name>/component.toml`, in folder-name order.
///
/// Folders without `component.toml` and hidden folders are skipped silently.
/// A component whose files resolve outside `root` is rejected.
///
/// # Errors
/// Fails only when the layer folder itself cannot be listed.
pub fn scan_directory(
    root: &Path,
    layer: &Path,
) -> io::Result<(Vec<LoadedComponent>, Vec<ScanProblem>)> {
    let mut entries = fs::read_dir(layer)?
        .filter_map(Result::ok)
        .filter(|entry| {
            !entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with('.'))
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();

    let mut components = Vec::new();
    let mut problems = Vec::new();
    for directory in entries {
        let definition_path = directory.join(DEFINITION_FILE);
        if fs::symlink_metadata(&definition_path).is_err() {
            continue;
        }
        let name = directory
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let source = match read_confined(root, &definition_path) {
            Ok(Some(source)) => source,
            Ok(None) => continue,
            Err(problem) => {
                problems.push(problem);
                continue;
            }
        };
        let definition = match ComponentDefinition::from_toml(&source) {
            Ok(definition) => definition,
            Err(error) => {
                problems.push(ScanProblem {
                    code: "library.invalid_definition",
                    severity: DiagnosticSeverity::Error,
                    message: format!("component `{name}` is invalid: {error}"),
                    path: definition_path,
                });
                continue;
            }
        };
        let documentation = match read_confined(root, &directory.join(DOCUMENTATION_FILE)) {
            Ok(documentation) => documentation,
            Err(mut problem) => {
                problem.severity = DiagnosticSeverity::Warning;
                problem.code = "library.documentation_unavailable";
                problems.push(problem);
                None
            }
        };
        components.push(LoadedComponent {
            definition,
            documentation,
            directory: Some(fs::canonicalize(&directory).unwrap_or(directory)),
            name,
        });
    }
    Ok((components, problems))
}

/// Reads a UTF-8 file that must resolve inside `root`. A missing file is
/// `Ok(None)`.
fn read_confined(root: &Path, path: &Path) -> Result<Option<String>, ScanProblem> {
    let problem = |code, message: String| ScanProblem {
        code,
        severity: DiagnosticSeverity::Error,
        message,
        path: path.to_path_buf(),
    };
    let canonical = match fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(problem(
                "library.component_unreadable",
                format!("{} cannot be read: {error}", path.display()),
            ));
        }
    };
    if !canonical.starts_with(root) {
        return Err(problem(
            "library.component_outside_root",
            format!(
                "{} resolves outside the layer's root and is ignored",
                path.display()
            ),
        ));
    }
    if !canonical.is_file() {
        return Err(problem(
            "library.component_unreadable",
            format!("{} is not a regular file", path.display()),
        ));
    }
    fs::read_to_string(&canonical).map(Some).map_err(|error| {
        problem(
            "library.component_unreadable",
            format!("{} cannot be read: {error}", path.display()),
        )
    })
}
