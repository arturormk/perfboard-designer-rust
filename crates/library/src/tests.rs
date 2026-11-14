use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use perfboard_core::DiagnosticSeverity;

use super::*;

struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "perfboard-library-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn definition(id: &str, version: &str) -> String {
    format!(
        r#"schema_version = 1
id = "{id}"
definition_version = "{version}"
name = "Test part"
origin_x = 0
origin_y = 0
allowed_rotations = ["deg0"]

[body]
x = 0.0
y = 0.0
width = 2.54
height = 2.54

[[pins]]
id = "1"
number = "1"
name = "Pin 1"
x = 0
y = 0
"#
    )
}

fn write_component(layer: &Path, folder: &str, id: &str, version: &str) {
    let directory = layer.join(folder);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("component.toml"), definition(id, version)).unwrap();
}

fn embedded(parts: &[(&str, &str)]) -> StandardLibrary {
    StandardLibrary::Embedded {
        version: "1.0.0".into(),
        components: parts
            .iter()
            .map(|(name, id)| StandardComponentSource {
                name: (*name).into(),
                definition: definition(id, "1.0.0"),
                documentation: Some(format!("# {name}")),
            })
            .collect(),
    }
}

fn no_standard() -> StandardLibrary {
    embedded(&[])
}

fn paths(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|entry| (*entry).to_owned()).collect()
}

fn ids(library: &ResolvedLibrary) -> Vec<&str> {
    library
        .definitions()
        .map(|definition| definition.id.as_str())
        .collect()
}

fn codes(library: &ResolvedLibrary) -> Vec<&'static str> {
    library
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn derives_the_project_stem() {
    assert_eq!(
        project_stem(Path::new("/a/nano-rs485.perfboard.json")).as_deref(),
        Some("nano-rs485")
    );
    assert_eq!(
        project_stem(Path::new("board.json")).as_deref(),
        Some("board")
    );
    assert_eq!(project_stem(Path::new("board")).as_deref(), Some("board"));
}

#[test]
fn default_layer_is_the_folder_named_after_the_project() {
    let dir = TestDir::new("default");
    let project = dir.path("nano.perfboard.json");
    write_component(&dir.path("nano.perfboard"), "part", "local.part", "1.0.0");
    let library = resolve(
        None,
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &embedded(&[("bridge", "dev.perfboard.bridge")]),
    );
    assert_eq!(ids(&library), ["local.part", "dev.perfboard.bridge"]);
    assert_eq!(library.layers.len(), 2);
    assert_eq!(library.layers[0].kind, LayerKind::Project);
    assert_eq!(
        library.layers[0].template.as_deref(),
        Some(DEFAULT_PROJECT_LAYER)
    );
    assert_eq!(library.layers[0].path, Some(dir.path("nano.perfboard")));
    assert_eq!(library.layers[0].status, LayerStatus::Loaded);
    assert_eq!(library.layers[0].component_count, 1);
    assert_eq!(library.layers[1].kind, LayerKind::Standard);
    assert_eq!(library.layers[1].version.as_deref(), Some("1.0.0"));
    assert_eq!(library.components[0].layer, 0);
    assert_eq!(
        library.components[0].directory,
        Some(dir.path("nano.perfboard/part"))
    );
    assert_eq!(library.diagnostics, [] as [LibraryDiagnostic; 0]);
}

#[test]
fn a_missing_default_folder_is_silent_but_a_listed_one_warns() {
    let dir = TestDir::new("missing");
    let project = dir.path("p.perfboard.json");
    let context = ResolveContext {
        project_file: Some(&project),
        user_components: None,
    };

    let implicit = resolve(None, &context, &no_standard());
    assert_eq!(implicit.layers[0].status, LayerStatus::Missing);
    assert_eq!(implicit.diagnostics, [] as [LibraryDiagnostic; 0]);

    let explicit = resolve(
        Some(&paths(&[DEFAULT_PROJECT_LAYER])),
        &context,
        &no_standard(),
    );
    assert_eq!(explicit.layers[0].status, LayerStatus::Missing);
    assert_eq!(codes(&explicit), ["library.layer_missing"]);
    assert_eq!(
        explicit.diagnostics[0].severity,
        DiagnosticSeverity::Warning
    );
}

#[test]
fn an_empty_list_leaves_only_the_standard_library() {
    let dir = TestDir::new("empty");
    let project = dir.path("p.perfboard.json");
    write_component(&dir.path("p.perfboard"), "part", "local.part", "1.0.0");
    let library = resolve(
        Some(&[]),
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &embedded(&[("bridge", "dev.perfboard.bridge")]),
    );
    assert_eq!(ids(&library), ["dev.perfboard.bridge"]);
    assert_eq!(library.layers.len(), 1);
}

#[test]
fn an_unsaved_project_has_inactive_project_layers() {
    let user = TestDir::new("unsaved-user");
    write_component(&user.0, "part", "local.user-part", "1.0.0");
    let library = resolve(
        Some(&paths(&[DEFAULT_PROJECT_LAYER, "${USER_COMPONENTS}"])),
        &ResolveContext {
            project_file: None,
            user_components: Some(&user.0),
        },
        &no_standard(),
    );
    assert_eq!(library.layers[0].status, LayerStatus::Inactive);
    assert_eq!(library.layers[1].kind, LayerKind::User);
    assert_eq!(library.layers[1].status, LayerStatus::Loaded);
    assert_eq!(ids(&library), ["local.user-part"]);
    assert_eq!(library.diagnostics, [] as [LibraryDiagnostic; 0]);
}

#[test]
fn a_user_layer_without_a_grant_is_an_error_and_others_still_load() {
    let dir = TestDir::new("ungranted");
    let project = dir.path("p.perfboard.json");
    write_component(&dir.path("p.perfboard"), "part", "local.part", "1.0.0");
    let library = resolve(
        Some(&paths(&[DEFAULT_PROJECT_LAYER, "${USER_COMPONENTS}/rs485"])),
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &no_standard(),
    );
    assert_eq!(library.layers[1].status, LayerStatus::NotGranted);
    assert_eq!(codes(&library), ["library.not_granted"]);
    assert!(library.has_errors());
    assert_eq!(ids(&library), ["local.part"]);
}

#[test]
fn a_grant_to_a_missing_folder_is_an_error() {
    let dir = TestDir::new("grant-missing");
    let gone = dir.path("gone");
    let library = resolve(
        Some(&paths(&["${USER_COMPONENTS}"])),
        &ResolveContext {
            project_file: None,
            user_components: Some(&gone),
        },
        &no_standard(),
    );
    assert_eq!(library.layers[0].status, LayerStatus::Missing);
    assert_eq!(codes(&library), ["library.user_components_unavailable"]);
    assert_eq!(library.diagnostics[0].path, Some(gone));
}

#[test]
fn malformed_entries_are_invalid_layers() {
    let dir = TestDir::new("malformed");
    let project = dir.path("p.perfboard.json");
    write_component(&dir.path("parts"), "part", "local.part", "1.0.0");
    let library = resolve(
        Some(&paths(&[
            "${PROJECT_DIR}/../elsewhere",
            "/abs/path",
            "~/parts",
            "${HOME}/parts",
            "${PROJECT_DIR}/parts",
        ])),
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &no_standard(),
    );
    assert_eq!(
        library
            .layers
            .iter()
            .map(|layer| layer.status)
            .collect::<Vec<_>>(),
        [
            LayerStatus::Invalid,
            LayerStatus::Invalid,
            LayerStatus::Invalid,
            LayerStatus::Invalid,
            LayerStatus::Loaded,
            LayerStatus::Loaded,
        ]
    );
    assert_eq!(library.layers[1].template.as_deref(), Some("/abs/path"));
    assert_eq!(codes(&library), ["library.invalid_path"; 4]);
    assert_eq!(ids(&library), ["local.part"]);
}

#[test]
fn a_stem_that_is_not_a_folder_name_is_invalid() {
    let dir = TestDir::new("bad-stem");
    // `${PROJECT_STEM}` of this file is `..`.
    let project = dir.path("...perfboard.json");
    let library = resolve(
        Some(&paths(&["${PROJECT_DIR}/${PROJECT_STEM}"])),
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &no_standard(),
    );
    assert_eq!(library.layers[0].status, LayerStatus::Invalid);
    assert_eq!(codes(&library), ["library.invalid_path"]);
}

#[test]
fn the_first_layer_defining_an_id_wins_and_records_shadows() {
    let dir = TestDir::new("shadow");
    let user = TestDir::new("shadow-user");
    let project = dir.path("p.perfboard.json");
    write_component(
        &dir.path("p.perfboard"),
        "bridge",
        "dev.perfboard.bridge",
        "9.0.0",
    );
    write_component(&user.0, "bridge", "dev.perfboard.bridge", "2.0.0");
    write_component(&user.0, "led", "local.led", "1.0.0");
    let library = resolve(
        Some(&paths(&[DEFAULT_PROJECT_LAYER, "${USER_COMPONENTS}"])),
        &ResolveContext {
            project_file: Some(&project),
            user_components: Some(&user.0),
        },
        &embedded(&[
            ("bridge", "dev.perfboard.bridge"),
            ("pin", "dev.perfboard.pin"),
        ]),
    );
    assert_eq!(
        ids(&library),
        ["dev.perfboard.bridge", "local.led", "dev.perfboard.pin"]
    );
    let bridge = library.get("dev.perfboard.bridge").unwrap();
    assert_eq!(bridge.layer, 0);
    assert_eq!(bridge.definition.definition_version, "9.0.0");
    assert_eq!(
        bridge.shadows,
        [
            ShadowedComponent {
                layer: 1,
                directory: Some(user.path("bridge")),
                definition_version: "2.0.0".into(),
            },
            ShadowedComponent {
                layer: 2,
                directory: None,
                definition_version: "1.0.0".into(),
            },
        ]
    );
    assert_eq!(library.layers[1].component_count, 2);
    assert_eq!(library.layers[2].component_count, 2);
    assert_eq!(library.diagnostics, [] as [LibraryDiagnostic; 0]);
}

#[test]
fn duplicate_ids_in_one_layer_load_neither() {
    let dir = TestDir::new("duplicate-id");
    let project = dir.path("p.perfboard.json");
    let layer = dir.path("p.perfboard");
    write_component(&layer, "a", "local.same", "1.0.0");
    write_component(&layer, "b", "local.same", "2.0.0");
    write_component(&layer, "c", "local.other", "1.0.0");
    let library = resolve(
        None,
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &embedded(&[("same", "local.same")]),
    );
    assert_eq!(codes(&library), ["library.duplicate_id"]);
    assert!(library.diagnostics[0].message.contains("a, b"));
    assert_eq!(library.layers[0].component_count, 1);
    // With both project copies dropped, the standard one is the winner.
    assert_eq!(ids(&library), ["local.other", "local.same"]);
    assert_eq!(library.get("local.same").unwrap().layer, 1);
}

#[test]
fn invalid_definitions_are_reported_and_skipped() {
    let dir = TestDir::new("invalid-definition");
    let project = dir.path("p.perfboard.json");
    let layer = dir.path("p.perfboard");
    write_component(&layer, "good", "local.good", "1.0.0");
    fs::create_dir_all(layer.join("bad")).unwrap();
    fs::write(layer.join("bad/component.toml"), "not = [valid").unwrap();
    let library = resolve(
        None,
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &no_standard(),
    );
    assert_eq!(codes(&library), ["library.invalid_definition"]);
    assert_eq!(
        library.diagnostics[0].path,
        Some(layer.join("bad/component.toml"))
    );
    assert_eq!(library.layers[0].status, LayerStatus::Loaded);
    assert_eq!(ids(&library), ["local.good"]);
}

#[test]
fn skips_hidden_folders_plain_files_and_folders_without_a_definition() {
    let dir = TestDir::new("skips");
    let project = dir.path("p.perfboard.json");
    let layer = dir.path("p.perfboard");
    write_component(&layer, ".hidden", "local.hidden", "1.0.0");
    write_component(&layer, "real", "local.real", "1.0.0");
    fs::create_dir_all(layer.join("empty")).unwrap();
    fs::write(layer.join("notes.txt"), "x").unwrap();
    let library = resolve(
        None,
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &no_standard(),
    );
    assert_eq!(ids(&library), ["local.real"]);
    assert_eq!(library.diagnostics, [] as [LibraryDiagnostic; 0]);
}

#[test]
fn loads_documentation_next_to_the_definition() {
    let dir = TestDir::new("docs");
    let project = dir.path("p.perfboard.json");
    let layer = dir.path("p.perfboard");
    write_component(&layer, "part", "local.part", "1.0.0");
    fs::write(layer.join("part/README.md"), "# Part").unwrap();
    let library = resolve(
        None,
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &embedded(&[("bridge", "dev.perfboard.bridge")]),
    );
    assert_eq!(
        library.get("local.part").unwrap().documentation.as_deref(),
        Some("# Part")
    );
    assert_eq!(
        library
            .get("dev.perfboard.bridge")
            .unwrap()
            .documentation
            .as_deref(),
        Some("# bridge")
    );
}

#[test]
fn a_file_in_place_of_a_layer_folder_is_invalid() {
    let dir = TestDir::new("not-dir");
    let project = dir.path("p.perfboard.json");
    fs::write(dir.path("p.perfboard"), "x").unwrap();
    let library = resolve(
        None,
        &ResolveContext {
            project_file: Some(&project),
            user_components: None,
        },
        &no_standard(),
    );
    assert_eq!(library.layers[0].status, LayerStatus::Invalid);
    assert_eq!(codes(&library), ["library.layer_not_directory"]);
}

#[test]
fn the_same_folder_listed_twice_is_read_once() {
    let dir = TestDir::new("duplicate-layer");
    let project = dir.path("p.perfboard.json");
    write_component(&dir.path("parts"), "part", "local.part", "1.0.0");
    let library = resolve(
        Some(&paths(&["${PROJECT_DIR}/parts", "${USER_COMPONENTS}"])),
        &ResolveContext {
            project_file: Some(&project),
            user_components: Some(&dir.path("parts")),
        },
        &no_standard(),
    );
    assert_eq!(library.layers[1].status, LayerStatus::Duplicate);
    assert_eq!(codes(&library), ["library.duplicate_layer"]);
    assert_eq!(
        library.get("local.part").unwrap().shadows,
        [] as [ShadowedComponent; 0]
    );
}

#[test]
fn a_standard_library_directory_replaces_the_embedded_copy() {
    let standard = TestDir::new("standard");
    write_component(&standard.0, "bridge", "dev.perfboard.bridge", "3.0.0");
    let library = resolve(
        Some(&[]),
        &ResolveContext::default(),
        &StandardLibrary::Directory(standard.0.clone()),
    );
    assert_eq!(library.layers[0].kind, LayerKind::Standard);
    assert_eq!(library.layers[0].path, Some(standard.0.clone()));
    assert_eq!(library.layers[0].version, None);
    assert_eq!(
        library
            .get("dev.perfboard.bridge")
            .unwrap()
            .definition
            .definition_version,
        "3.0.0"
    );

    let gone = standard.path("gone");
    let missing = resolve(
        Some(&[]),
        &ResolveContext::default(),
        &StandardLibrary::Directory(gone),
    );
    assert_eq!(missing.layers[0].status, LayerStatus::Missing);
    assert_eq!(codes(&missing), ["library.standard_unavailable"]);
}

#[test]
fn an_invalid_embedded_definition_is_reported() {
    let library = resolve(
        Some(&[]),
        &ResolveContext::default(),
        &StandardLibrary::Embedded {
            version: "1.0.0".into(),
            components: vec![StandardComponentSource {
                name: "broken".into(),
                definition: "x = [".into(),
                documentation: None,
            }],
        },
    );
    assert_eq!(codes(&library), ["library.invalid_definition"]);
    assert_eq!(library.components, [] as [ResolvedComponent; 0]);
}

#[test]
fn the_repository_components_load_as_a_standard_directory() {
    let components = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../components");
    let library = resolve(
        Some(&[]),
        &ResolveContext::default(),
        &StandardLibrary::Directory(components),
    );
    assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
    assert!(library.get("dev.perfboard.bridge").is_some());
}

#[test]
fn serializes_kinds_and_statuses_in_snake_case() {
    let library = resolve(
        Some(&paths(&["${USER_COMPONENTS}"])),
        &ResolveContext::default(),
        &no_standard(),
    );
    let json = serde_json::to_value(&library).unwrap();
    assert_eq!(json["layers"][0]["kind"], "user");
    assert_eq!(json["layers"][0]["status"], "not_granted");
    assert_eq!(json["layers"][1]["kind"], "standard");
    assert_eq!(json["diagnostics"][0]["severity"], "error");
}

#[cfg(unix)]
mod symlinks {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn a_layer_symlinked_outside_its_root_is_rejected() {
        let dir = TestDir::new("layer-escape");
        let outside = TestDir::new("layer-escape-outside");
        write_component(&outside.0, "part", "local.secret", "1.0.0");
        let project = dir.path("p.perfboard.json");
        symlink(&outside.0, dir.path("p.perfboard")).unwrap();
        let library = resolve(
            None,
            &ResolveContext {
                project_file: Some(&project),
                user_components: None,
            },
            &no_standard(),
        );
        assert_eq!(library.layers[0].status, LayerStatus::Invalid);
        assert_eq!(codes(&library), ["library.layer_outside_root"]);
        assert_eq!(library.components, [] as [ResolvedComponent; 0]);
    }

    #[test]
    fn a_component_symlinked_outside_its_root_is_rejected() {
        let dir = TestDir::new("component-escape");
        let outside = TestDir::new("component-escape-outside");
        write_component(&outside.0, "part", "local.secret", "1.0.0");
        let project = dir.path("p.perfboard.json");
        let layer = dir.path("p.perfboard");
        write_component(&layer, "good", "local.good", "1.0.0");
        symlink(outside.path("part"), layer.join("linked")).unwrap();
        let library = resolve(
            None,
            &ResolveContext {
                project_file: Some(&project),
                user_components: None,
            },
            &no_standard(),
        );
        assert_eq!(codes(&library), ["library.component_outside_root"]);
        assert_eq!(ids(&library), ["local.good"]);
    }

    #[test]
    fn documentation_symlinked_outside_its_root_is_dropped_with_a_warning() {
        let dir = TestDir::new("docs-escape");
        let outside = TestDir::new("docs-escape-outside");
        fs::write(outside.path("secret.md"), "secret").unwrap();
        let project = dir.path("p.perfboard.json");
        let layer = dir.path("p.perfboard");
        write_component(&layer, "part", "local.part", "1.0.0");
        symlink(outside.path("secret.md"), layer.join("part/README.md")).unwrap();
        let library = resolve(
            None,
            &ResolveContext {
                project_file: Some(&project),
                user_components: None,
            },
            &no_standard(),
        );
        assert_eq!(codes(&library), ["library.documentation_unavailable"]);
        assert_eq!(library.diagnostics[0].severity, DiagnosticSeverity::Warning);
        assert_eq!(library.get("local.part").unwrap().documentation, None);
    }

    #[test]
    fn symlinks_that_stay_inside_the_root_are_allowed() {
        let dir = TestDir::new("inside");
        let project = dir.path("p.perfboard.json");
        write_component(&dir.path("real"), "part", "local.part", "1.0.0");
        symlink(dir.path("real"), dir.path("p.perfboard")).unwrap();
        let library = resolve(
            None,
            &ResolveContext {
                project_file: Some(&project),
                user_components: None,
            },
            &no_standard(),
        );
        assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
        assert_eq!(library.layers[0].path, Some(dir.path("real")));
        assert_eq!(ids(&library), ["local.part"]);
    }

    #[test]
    fn a_user_library_reached_through_a_symlink_is_confined_to_its_target() {
        let target = TestDir::new("user-target");
        let links = TestDir::new("user-links");
        write_component(&target.0, "part", "local.part", "1.0.0");
        symlink(&target.0, links.path("parts")).unwrap();
        let library = resolve(
            Some(&paths(&["${USER_COMPONENTS}"])),
            &ResolveContext {
                project_file: None,
                user_components: Some(&links.path("parts")),
            },
            &no_standard(),
        );
        assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
        assert_eq!(library.layers[0].path, Some(target.0.clone()));
        assert_eq!(ids(&library), ["local.part"]);
    }
}
