//! The component library offered for new placements (ADR 0014): the open
//! project's layers, then the standard library.
//!
//! Release builds use the copy compiled into the binary, so packages and
//! hand-copied executables need nothing installed beside them. Debug builds
//! read the checkout's `components/`, so edits show without rebuilding.
//! `PERFBOARD_STANDARD_COMPONENTS` replaces either with a folder.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use perfboard_library::{
    ResolveContext, ResolvedLibrary, StandardComponentSource, StandardLibrary, resolve,
};
use perfboard_project::ProjectSession;

/// The environment variable that replaces the standard library with a folder.
pub const OVERRIDE_VARIABLE: &str = "PERFBOARD_STANDARD_COMPONENTS";

struct EmbeddedSource {
    name: &'static str,
    definition: &'static str,
    documentation: Option<&'static str>,
}

const EMBEDDED: &[EmbeddedSource] = include!(concat!(env!("OUT_DIR"), "/standard_components.rs"));

fn checkout_components() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../components")
}

fn embedded() -> StandardLibrary {
    StandardLibrary::Embedded {
        version: env!("PERFBOARD_VERSION").to_owned(),
        components: EMBEDDED
            .iter()
            .map(|source| StandardComponentSource {
                name: source.name.to_owned(),
                definition: source.definition.to_owned(),
                documentation: source.documentation.map(str::to_owned),
            })
            .collect(),
    }
}

fn select(override_dir: Option<OsString>, debug: bool, checkout: PathBuf) -> StandardLibrary {
    if let Some(directory) = override_dir.filter(|value| !value.is_empty()) {
        return StandardLibrary::Directory(directory.into());
    }
    if debug && checkout.is_dir() {
        return StandardLibrary::Directory(checkout);
    }
    embedded()
}

/// The standard library for this process.
pub fn standard_library() -> StandardLibrary {
    select(
        std::env::var_os(OVERRIDE_VARIABLE),
        cfg!(debug_assertions),
        checkout_components(),
    )
}

/// What resolution needs from the session, copied out so that no folder is
/// read while the session is locked.
pub struct LibraryInputs {
    component_paths: Option<Vec<String>>,
    project_file: Option<PathBuf>,
}

impl LibraryInputs {
    pub fn of(session: &ProjectSession) -> Self {
        Self {
            component_paths: session.document().component_paths.clone(),
            project_file: session.file_path().map(Path::to_path_buf),
        }
    }
}

/// The library offered for new placements in the open project.
///
/// No user library is granted yet (step 5 of the plan), so `${USER_COMPONENTS}`
/// layers report "not granted". Diagnostics are also logged; the definitions
/// that loaded are offered either way.
pub fn current_library(inputs: &LibraryInputs) -> ResolvedLibrary {
    let library = resolve(
        inputs.component_paths.as_deref(),
        &ResolveContext {
            project_file: inputs.project_file.as_deref(),
            user_components: None,
        },
        &standard_library(),
    );
    for diagnostic in &library.diagnostics {
        eprintln!(
            "Component library: {} ({}){}",
            diagnostic.message,
            diagnostic.code,
            diagnostic
                .path
                .as_ref()
                .map(|path| format!(": {}", path.display()))
                .unwrap_or_default()
        );
    }
    library
}

#[cfg(test)]
mod tests {
    use std::fs;

    use perfboard_core::{BoardSize, ProjectId};
    use perfboard_library::{LayerKind, LayerStatus};
    use perfboard_project::{Board, ProjectDocument};

    use super::*;

    fn session(paths: Option<Vec<String>>, file: Option<PathBuf>) -> ProjectSession {
        let mut document = ProjectDocument::new(
            ProjectId::new(),
            "Library",
            Board::new("board", BoardSize::new(10, 10).unwrap(), 2.54),
        );
        document.component_paths = paths;
        let mut session = ProjectSession::new(document);
        if let Some(file) = file {
            session.set_file_path(file);
        }
        session
    }

    #[test]
    fn a_saved_project_puts_its_own_folder_before_the_standard_library() {
        let dir =
            std::env::temp_dir().join(format!("perfboard-desktop-library-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let part = dir.join("board.perfboard/bridge");
        fs::create_dir_all(&part).unwrap();
        let bridge = fs::read_to_string(checkout_components().join("bridge/component.toml"))
            .unwrap()
            .replace(
                "definition_version = \"1.0.0\"",
                "definition_version = \"7.0.0\"",
            );
        fs::write(part.join("component.toml"), bridge).unwrap();

        let unsaved = current_library(&LibraryInputs::of(&session(None, None)));
        assert_eq!(unsaved.layers[0].status, LayerStatus::Inactive);
        assert_eq!(unsaved.get("dev.perfboard.bridge").unwrap().layer, 1);

        let saved = current_library(&LibraryInputs::of(&session(
            None,
            Some(dir.join("board.perfboard.json")),
        )));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(saved.layers[0].kind, LayerKind::Project);
        assert_eq!(saved.layers[0].status, LayerStatus::Loaded);
        let winner = saved.get("dev.perfboard.bridge").unwrap();
        assert_eq!(winner.layer, 0);
        assert_eq!(winner.definition.definition_version, "7.0.0");
        assert_eq!(winner.shadows.len(), 1);
        assert_eq!(winner.shadows[0].layer, 1);
    }

    #[test]
    fn the_example_project_finds_its_own_parts() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/nano-rs485.perfboard.json")
            .canonicalize()
            .unwrap();
        let document = perfboard_project::load_project(&file).unwrap();
        let mut session = ProjectSession::new(document);
        session.set_file_path(file);
        let library = current_library(&LibraryInputs::of(&session));
        assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
        assert_eq!(library.layers[0].kind, LayerKind::Project);
        assert_eq!(library.layers[0].component_count, 4);
        for instance in session.document().design.components.values() {
            let component = library
                .get(&instance.definition_id)
                .unwrap_or_else(|| panic!("{} is not in the library", instance.definition_id));
            assert!(
                instance.definition_id.starts_with("local.")
                    || library.layers[component.layer].kind == LayerKind::Standard,
                "{} comes from the wrong layer",
                instance.definition_id
            );
        }
        assert!(library.get("local.rj45-connector").is_some());
        assert!(library.get("dev.perfboard.bridge").is_some());
    }

    #[test]
    fn user_layers_are_not_granted_yet() {
        let library = current_library(&LibraryInputs::of(&session(
            Some(vec!["${USER_COMPONENTS}".into()]),
            None,
        )));
        assert_eq!(library.layers[0].status, LayerStatus::NotGranted);
        assert_eq!(library.layers[1].kind, LayerKind::Standard);
        assert!(library.get("dev.perfboard.bridge").is_some());
    }

    #[test]
    fn embeds_every_checkout_component_without_diagnostics() {
        let library = resolve(Some(&[]), &ResolveContext::default(), &embedded());
        assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
        let folders = fs::read_dir(checkout_components())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().join("component.toml").is_file())
            .count();
        assert_eq!(EMBEDDED.len(), folders);
        assert_eq!(library.components.len(), folders);
        assert_eq!(
            library.layers[0].version.as_deref(),
            Some(env!("PERFBOARD_VERSION"))
        );
        let bridge = library.get("dev.perfboard.bridge").unwrap();
        assert!(bridge.documentation.is_some());
    }

    #[test]
    fn the_override_wins_then_the_checkout_in_debug_then_the_embedded_copy() {
        let checkout = checkout_components();
        assert_eq!(
            select(Some("/opt/parts".into()), true, checkout.clone()),
            StandardLibrary::Directory("/opt/parts".into())
        );
        assert_eq!(
            select(Some("/opt/parts".into()), false, checkout.clone()),
            StandardLibrary::Directory("/opt/parts".into())
        );
        assert_eq!(
            select(None, true, checkout.clone()),
            StandardLibrary::Directory(checkout.clone())
        );
        assert_eq!(select(Some(OsString::new()), false, checkout), embedded());
        assert_eq!(
            select(None, true, PathBuf::from("/nonexistent/components")),
            embedded()
        );
    }

    #[test]
    fn the_checkout_and_embedded_copies_agree() {
        let from_checkout = resolve(
            Some(&[]),
            &ResolveContext::default(),
            &StandardLibrary::Directory(checkout_components()),
        );
        let from_binary = resolve(Some(&[]), &ResolveContext::default(), &embedded());
        assert_eq!(from_checkout.layers[0].status, LayerStatus::Loaded);
        assert_eq!(
            from_checkout.definitions().collect::<Vec<_>>(),
            from_binary.definitions().collect::<Vec<_>>()
        );
    }
}
