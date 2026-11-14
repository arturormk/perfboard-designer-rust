//! Layered component library resolution (ADR 0014).
//!
//! A project's library is its `component_paths` layers, in order, followed by
//! the standard library. The first layer that defines an `id` wins; later
//! definitions with that `id` are reported as shadowed. Every layer path is
//! confined to the root it names, symlinks included.

mod scan;
mod template;

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use perfboard_core::{ComponentDefinition, DiagnosticSeverity};
use serde::Serialize;

pub use template::{
    DEFAULT_PROJECT_LAYER, FolderTemplateError, LayerRoot, PROJECT_DIR, PROJECT_STEM, PathTemplate,
    PathTemplateError, USER_COMPONENTS, template_for_folder,
};

use scan::{LoadedComponent, scan_directory};

/// The file extension of project files, stripped to form `${PROJECT_STEM}`.
pub const PROJECT_FILE_SUFFIX: &str = ".perfboard.json";

/// The `id` prefix reserved for the standard library.
pub const STANDARD_ID_PREFIX: &str = "dev.perfboard.";

/// One standard-library component supplied in memory, for example compiled
/// into the binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardComponentSource {
    /// The component's folder name, used for ordering and diagnostics.
    pub name: String,
    /// The `component.toml` source.
    pub definition: String,
    /// The `README.md` source, if any.
    pub documentation: Option<String>,
}

/// Where the standard library comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StandardLibrary {
    /// Components compiled into the application.
    Embedded {
        version: String,
        components: Vec<StandardComponentSource>,
    },
    /// A directory replacing the embedded copy (development or override).
    Directory(PathBuf),
}

/// Machine-specific inputs to resolution.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolveContext<'a> {
    /// The project file, or `None` for a project that has never been saved.
    pub project_file: Option<&'a Path>,
    /// The user library granted to this project, if any.
    pub user_components: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    Project,
    User,
    Standard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerStatus {
    /// The layer was read; individual components may still have diagnostics.
    Loaded,
    /// The layer's folder does not exist.
    Missing,
    /// The layer needs `${USER_COMPONENTS}` and the project has no grant.
    NotGranted,
    /// The layer needs `${PROJECT_DIR}` and the project has not been saved.
    Inactive,
    /// The layer resolves to the same folder as an earlier layer and is skipped.
    Duplicate,
    /// The entry is malformed, escapes its root, or cannot be read.
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Layer {
    pub kind: LayerKind,
    /// The `component_paths` entry, or `None` for the standard library.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The canonical folder, when it could be resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The standard library's version, when it is the embedded copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: LayerStatus,
    /// Definitions loaded from this layer, shadowed ones included.
    pub component_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LibraryDiagnostic {
    pub code: &'static str,
    pub severity: DiagnosticSeverity,
    pub message: String,
    /// Index into [`ResolvedLibrary::layers`].
    pub layer: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShadowedComponent {
    pub layer: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<PathBuf>,
    pub definition_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedComponent {
    pub definition: ComponentDefinition,
    /// Untrusted Markdown, served on request rather than with the library.
    #[serde(skip)]
    pub documentation: Option<String>,
    /// Index into [`ResolvedLibrary::layers`] of the layer that won.
    pub layer: usize,
    /// The component folder; `None` for embedded standard components.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<PathBuf>,
    /// Definitions with the same `id` in later layers, in layer order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shadows: Vec<ShadowedComponent>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ResolvedLibrary {
    pub layers: Vec<Layer>,
    /// Winning definitions, ordered by layer and then by folder name.
    pub components: Vec<ResolvedComponent>,
    pub diagnostics: Vec<LibraryDiagnostic>,
}

impl ResolvedLibrary {
    /// The winning definition for `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ResolvedComponent> {
        self.components
            .iter()
            .find(|component| component.definition.id == id)
    }

    /// The winning definitions, in library order.
    pub fn definitions(&self) -> impl Iterator<Item = &ComponentDefinition> {
        self.components
            .iter()
            .map(|component| &component.definition)
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}

/// `${PROJECT_STEM}` for a project file: its name without `.perfboard.json`
/// (or `.json`).
#[must_use]
pub fn project_stem(project_file: &Path) -> Option<String> {
    let name = project_file.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(PROJECT_FILE_SUFFIX)
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name);
    Some(stem.to_owned())
}

/// Resolves a project's component library.
///
/// `component_paths` is the project's field: `None` when absent, which means
/// [`DEFAULT_PROJECT_LAYER`]. Resolution never fails as a whole; problems are
/// reported as layer statuses and diagnostics.
#[must_use]
pub fn resolve(
    component_paths: Option<&[String]>,
    context: &ResolveContext<'_>,
    standard: &StandardLibrary,
) -> ResolvedLibrary {
    let mut resolver = Resolver::default();
    let explicit = component_paths.is_some();
    let default_entries = [DEFAULT_PROJECT_LAYER.to_owned()];
    let entries = component_paths.unwrap_or(&default_entries);
    let roots = Roots::new(context);
    for entry in entries {
        resolver.project_entry(entry, explicit, context, &roots);
    }
    resolver.standard(standard);
    resolver.finish()
}

struct Roots {
    project_dir: Option<std::io::Result<PathBuf>>,
    user_components: Option<std::io::Result<PathBuf>>,
    stem: Option<String>,
}

impl Roots {
    fn new(context: &ResolveContext<'_>) -> Self {
        let canonical_dir = |path: &Path| {
            fs::canonicalize(path).and_then(|path| {
                if path.is_dir() {
                    Ok(path)
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotADirectory,
                        "not a directory",
                    ))
                }
            })
        };
        Self {
            project_dir: context.project_file.map(|file| {
                let parent = match file.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => parent,
                    _ => Path::new("."),
                };
                canonical_dir(parent)
            }),
            user_components: context.user_components.map(canonical_dir),
            stem: context.project_file.and_then(project_stem),
        }
    }
}

#[derive(Default)]
struct Resolver {
    layers: Vec<Layer>,
    diagnostics: Vec<LibraryDiagnostic>,
    loaded: Vec<(usize, Vec<LoadedComponent>)>,
    seen_paths: HashSet<PathBuf>,
}

impl Resolver {
    fn push_layer(&mut self, kind: LayerKind, template: Option<String>) -> usize {
        self.layers.push(Layer {
            kind,
            template,
            path: None,
            version: None,
            status: LayerStatus::Loaded,
            component_count: 0,
        });
        self.layers.len() - 1
    }

    fn report(
        &mut self,
        layer: usize,
        severity: DiagnosticSeverity,
        code: &'static str,
        message: impl Into<String>,
        path: Option<PathBuf>,
    ) {
        self.diagnostics.push(LibraryDiagnostic {
            code,
            severity,
            message: message.into(),
            layer,
            path,
        });
    }

    fn fail(&mut self, layer: usize, status: LayerStatus, code: &'static str, message: String) {
        self.layers[layer].status = status;
        self.report(layer, DiagnosticSeverity::Error, code, message, None);
    }

    fn project_entry(
        &mut self,
        entry: &str,
        explicit: bool,
        context: &ResolveContext<'_>,
        roots: &Roots,
    ) {
        let template = match PathTemplate::parse(entry) {
            Ok(template) => template,
            Err(error) => {
                let layer = self.push_layer(LayerKind::Project, Some(entry.to_owned()));
                self.fail(
                    layer,
                    LayerStatus::Invalid,
                    "library.invalid_path",
                    format!("component path `{entry}` is invalid: {error}"),
                );
                return;
            }
        };
        let kind = match template.root() {
            LayerRoot::ProjectDir => LayerKind::Project,
            LayerRoot::UserComponents => LayerKind::User,
        };
        let layer = self.push_layer(kind, Some(template.to_string()));

        let Some(root) = self.layer_root(layer, &template, context, roots) else {
            return;
        };

        let Some(relative) = template.relative_path(roots.stem.as_deref()) else {
            self.fail(
                layer,
                LayerStatus::Invalid,
                "library.invalid_path",
                format!("`{template}` does not form a valid folder name for this project file"),
            );
            return;
        };
        let candidate = root.join(relative);
        let path = match fs::canonicalize(&candidate) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.layers[layer].status = LayerStatus::Missing;
                if explicit {
                    self.report(
                        layer,
                        DiagnosticSeverity::Warning,
                        "library.layer_missing",
                        format!("the folder for `{template}` does not exist"),
                        Some(candidate),
                    );
                }
                return;
            }
            Err(error) => {
                self.layers[layer].status = LayerStatus::Invalid;
                self.report(
                    layer,
                    DiagnosticSeverity::Error,
                    "library.layer_unreadable",
                    format!("the folder for `{template}` cannot be read: {error}"),
                    Some(candidate),
                );
                return;
            }
        };
        if !path.starts_with(root) {
            self.layers[layer].status = LayerStatus::Invalid;
            self.report(
                layer,
                DiagnosticSeverity::Error,
                "library.layer_outside_root",
                format!(
                    "the folder for `{template}` resolves outside {}",
                    template.root().variable()
                ),
                Some(candidate),
            );
            return;
        }
        if !path.is_dir() {
            self.layers[layer].status = LayerStatus::Invalid;
            self.report(
                layer,
                DiagnosticSeverity::Error,
                "library.layer_not_directory",
                format!("`{template}` is not a folder"),
                Some(path),
            );
            return;
        }
        let root = root.clone();
        self.scan_layer(layer, &root, path);
    }

    /// The canonical root a layer is confined to, or `None` after recording
    /// why it is unavailable.
    fn layer_root<'r>(
        &mut self,
        layer: usize,
        template: &PathTemplate,
        context: &ResolveContext<'_>,
        roots: &'r Roots,
    ) -> Option<&'r PathBuf> {
        match template.root() {
            LayerRoot::ProjectDir => match &roots.project_dir {
                None => {
                    self.layers[layer].status = LayerStatus::Inactive;
                    None
                }
                Some(Err(error)) => {
                    let folder = context
                        .project_file
                        .and_then(Path::parent)
                        .map(Path::to_path_buf);
                    self.layers[layer].status = LayerStatus::Missing;
                    self.report(
                        layer,
                        DiagnosticSeverity::Error,
                        "library.project_dir_unavailable",
                        format!("the project folder is unavailable: {error}"),
                        folder,
                    );
                    None
                }
                Some(Ok(root)) => Some(root),
            },
            LayerRoot::UserComponents => match &roots.user_components {
                None => {
                    self.fail(
                        layer,
                        LayerStatus::NotGranted,
                        "library.not_granted",
                        format!(
                            "`{template}` needs a user library, and none is granted to this project"
                        ),
                    );
                    None
                }
                Some(Err(error)) => {
                    self.layers[layer].status = LayerStatus::Missing;
                    self.report(
                        layer,
                        DiagnosticSeverity::Error,
                        "library.user_components_unavailable",
                        format!("the user library granted to this project is unavailable: {error}"),
                        context.user_components.map(Path::to_path_buf),
                    );
                    None
                }
                Some(Ok(root)) => Some(root),
            },
        }
    }

    fn scan_layer(&mut self, layer: usize, root: &Path, path: PathBuf) {
        self.layers[layer].path = Some(path.clone());
        if !self.seen_paths.insert(path.clone()) {
            self.layers[layer].status = LayerStatus::Duplicate;
            self.report(
                layer,
                DiagnosticSeverity::Warning,
                "library.duplicate_layer",
                "this folder is already an earlier layer and is skipped",
                Some(path),
            );
            return;
        }
        match scan_directory(root, &path) {
            Ok((components, problems)) => {
                for problem in problems {
                    self.report(
                        layer,
                        problem.severity,
                        problem.code,
                        problem.message,
                        Some(problem.path),
                    );
                }
                self.add_components(layer, components);
            }
            Err(error) => {
                self.layers[layer].status = LayerStatus::Invalid;
                self.report(
                    layer,
                    DiagnosticSeverity::Error,
                    "library.layer_unreadable",
                    format!("the folder cannot be read: {error}"),
                    Some(path),
                );
            }
        }
    }

    fn standard(&mut self, standard: &StandardLibrary) {
        let layer = self.push_layer(LayerKind::Standard, None);
        match standard {
            StandardLibrary::Embedded {
                version,
                components,
            } => {
                self.layers[layer].version = Some(version.clone());
                let mut sources = components.iter().collect::<Vec<_>>();
                sources.sort_by(|left, right| left.name.cmp(&right.name));
                let mut loaded = Vec::new();
                for source in sources {
                    match ComponentDefinition::from_toml(&source.definition) {
                        Ok(definition) => loaded.push(LoadedComponent {
                            definition,
                            documentation: source.documentation.clone(),
                            directory: None,
                            name: source.name.clone(),
                        }),
                        Err(error) => self.report(
                            layer,
                            DiagnosticSeverity::Error,
                            "library.invalid_definition",
                            format!("standard component `{}` is invalid: {error}", source.name),
                            None,
                        ),
                    }
                }
                self.add_components(layer, loaded);
            }
            StandardLibrary::Directory(directory) => match fs::canonicalize(directory) {
                Ok(path) if path.is_dir() => self.scan_layer(layer, &path.clone(), path),
                Ok(path) => {
                    self.layers[layer].status = LayerStatus::Invalid;
                    self.report(
                        layer,
                        DiagnosticSeverity::Error,
                        "library.layer_not_directory",
                        "the standard library override is not a folder",
                        Some(path),
                    );
                }
                Err(error) => {
                    self.layers[layer].status = LayerStatus::Missing;
                    self.report(
                        layer,
                        DiagnosticSeverity::Error,
                        "library.standard_unavailable",
                        format!("the standard library folder is unavailable: {error}"),
                        Some(directory.clone()),
                    );
                }
            },
        }
    }

    /// Drops every definition whose `id` occurs more than once in the layer.
    fn add_components(&mut self, layer: usize, components: Vec<LoadedComponent>) {
        let mut by_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for component in &components {
            by_id
                .entry(component.definition.id.clone())
                .or_default()
                .push(component.name.clone());
        }
        let duplicates = by_id
            .into_iter()
            .filter(|(_, names)| names.len() > 1)
            .collect::<BTreeMap<_, _>>();
        for (id, names) in &duplicates {
            self.report(
                layer,
                DiagnosticSeverity::Error,
                "library.duplicate_id",
                format!(
                    "component id `{id}` is defined by {} in this layer; none of them is loaded",
                    names.join(", ")
                ),
                self.layers[layer].path.clone(),
            );
        }
        let components = components
            .into_iter()
            .filter(|component| !duplicates.contains_key(&component.definition.id))
            .collect::<Vec<_>>();
        self.layers[layer].component_count = components.len();
        self.loaded.push((layer, components));
    }

    /// Warns about project and user parts that take a reserved `id` without
    /// overriding a standard part: a later standard part could claim it.
    fn check_reserved_ids(&mut self) {
        let standard_ids = self
            .loaded
            .iter()
            .filter(|(layer, _)| self.layers[*layer].kind == LayerKind::Standard)
            .flat_map(|(_, components)| components)
            .map(|component| component.definition.id.clone())
            .collect::<HashSet<_>>();
        let mut warnings = Vec::new();
        for (layer, components) in &self.loaded {
            if self.layers[*layer].kind == LayerKind::Standard {
                continue;
            }
            for component in components {
                let id = &component.definition.id;
                if id.starts_with(STANDARD_ID_PREFIX) && !standard_ids.contains(id) {
                    warnings.push((
                        *layer,
                        format!(
                            "component id `{id}` uses the `{STANDARD_ID_PREFIX}` prefix, which is \
                             reserved for the standard library; use your own prefix or `local.`"
                        ),
                        component.directory.clone(),
                    ));
                }
            }
        }
        for (layer, message, path) in warnings {
            self.report(
                layer,
                DiagnosticSeverity::Warning,
                "library.reserved_id",
                message,
                path,
            );
        }
    }

    fn finish(mut self) -> ResolvedLibrary {
        self.check_reserved_ids();
        let mut components: Vec<ResolvedComponent> = Vec::new();
        let mut index: BTreeMap<String, usize> = BTreeMap::new();
        for (layer, loaded) in self.loaded {
            for component in loaded {
                if let Some(&winner) = index.get(&component.definition.id) {
                    components[winner].shadows.push(ShadowedComponent {
                        layer,
                        directory: component.directory,
                        definition_version: component.definition.definition_version,
                    });
                } else {
                    index.insert(component.definition.id.clone(), components.len());
                    components.push(ResolvedComponent {
                        definition: component.definition,
                        documentation: component.documentation,
                        layer,
                        directory: component.directory,
                        shadows: Vec::new(),
                    });
                }
            }
        }
        ResolvedLibrary {
            layers: self.layers,
            components,
            diagnostics: self.diagnostics,
        }
    }
}

#[cfg(test)]
mod tests;
