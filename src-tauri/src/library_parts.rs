//! Part designer access to the open project's libraries (ADR 0014): which
//! folders a part may be opened from, and creating or copying parts into a
//! project or user library. Standard parts are never edited in place.

use std::{
    fs,
    path::{Path, PathBuf},
};

use perfboard_core::ComponentDraft;
use perfboard_library::{
    LayerKind, LayerStatus, PathTemplate, ResolvedComponent, ResolvedLibrary, STANDARD_ID_PREFIX,
    project_stem,
};
use serde::Serialize;

use crate::component_library::LibraryInputs;

const DEFINITION_FILE: &str = "component.toml";
const DOCUMENTATION_FILE: &str = "README.md";

/// A part found in a folder the user picked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FolderPart {
    pub directory: PathBuf,
    pub id: String,
    pub name: String,
    pub pin_count: usize,
}

/// The parts directly inside `folder`, in folder-name order, or `None` when
/// it holds none (it may be a part folder itself, or unrelated).
pub fn folder_parts(folder: &Path) -> Option<Vec<FolderPart>> {
    let library = perfboard_library::resolve(
        Some(&[]),
        &perfboard_library::ResolveContext::default(),
        &perfboard_library::StandardLibrary::Directory(folder.to_path_buf()),
    );
    let mut parts = library
        .components
        .iter()
        .flat_map(|component| {
            let winner = component.directory.clone().map(|directory| FolderPart {
                directory,
                id: component.definition.id.clone(),
                name: component.definition.name.clone(),
                pin_count: component.definition.pins.len(),
            });
            winner.into_iter()
        })
        .collect::<Vec<_>>();
    parts.sort_by(|left, right| left.directory.cmp(&right.directory));
    (!parts.is_empty()).then_some(parts)
}

/// The folder a part may be opened from: a loaded project or user layer, or
/// the folder the user last picked. Returns the canonical layer folder, which
/// confines the part's files.
///
/// # Errors
/// Fails when `directory` is not directly inside one of those folders.
pub fn authorized_parent(
    library: &ResolvedLibrary,
    picked: Option<&Path>,
    directory: &Path,
) -> Result<PathBuf, String> {
    let directory = fs::canonicalize(directory)
        .map_err(|error| format!("{} is unavailable: {error}", directory.display()))?;
    let parent = directory
        .parent()
        .ok_or_else(|| format!("{} has no parent folder", directory.display()))?;
    let picked = picked.and_then(|path| fs::canonicalize(path).ok());
    let allowed = library
        .layers
        .iter()
        .filter(|layer| layer.kind != LayerKind::Standard && layer.status == LayerStatus::Loaded)
        .filter_map(|layer| layer.path.as_deref())
        .chain(picked.as_deref())
        .any(|folder| folder == parent);
    if allowed {
        Ok(parent.to_path_buf())
    } else {
        Err(format!(
            "{} is not in one of this project's editable libraries",
            directory.display()
        ))
    }
}

/// A project or user layer that parts can be written to.
#[derive(Debug, PartialEq, Eq)]
pub struct WritableLayer {
    pub index: usize,
    /// The layer folder, canonical. Created if it did not exist.
    pub path: PathBuf,
}

/// The first `kind` layer that can take new parts, creating its folder when
/// it does not exist yet (for example the default project folder).
///
/// # Errors
/// Explains why no such layer is available: none listed, the project is not
/// saved, no user library is granted, or the folder is unusable.
pub fn writable_layer(
    library: &ResolvedLibrary,
    inputs: &LibraryInputs,
    kind: LayerKind,
) -> Result<WritableLayer, String> {
    let what = match kind {
        LayerKind::Project => "project folder",
        LayerKind::User => "user library",
        LayerKind::Standard => return Err("the standard library is read-only".into()),
    };
    let mut reason =
        format!("This project's libraries list no {what}. Add one in Component libraries.");
    for (index, layer) in library.layers.iter().enumerate() {
        if layer.kind != kind {
            continue;
        }
        match layer.status {
            LayerStatus::Loaded => {
                if let Some(path) = &layer.path {
                    return Ok(WritableLayer {
                        index,
                        path: path.clone(),
                    });
                }
            }
            LayerStatus::Missing => {
                if let Some(path) = create_missing_layer(layer.template.as_deref(), inputs, kind)? {
                    return Ok(WritableLayer { index, path });
                }
            }
            LayerStatus::Inactive => {
                reason = "Save the project first: its own folders sit next to its file.".into();
            }
            LayerStatus::NotGranted => {
                reason =
                    "Grant a user library to this project first, in Component libraries.".into();
            }
            LayerStatus::Duplicate | LayerStatus::Invalid => {}
        }
    }
    Err(reason)
}

fn create_missing_layer(
    template: Option<&str>,
    inputs: &LibraryInputs,
    kind: LayerKind,
) -> Result<Option<PathBuf>, String> {
    let Some(template) = template.and_then(|source| PathTemplate::parse(source).ok()) else {
        return Ok(None);
    };
    let root = match kind {
        LayerKind::Project => inputs
            .project_file
            .as_deref()
            .and_then(Path::parent)
            .map(|parent| {
                if parent.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    parent
                }
            }),
        LayerKind::User => inputs.user_components.as_deref(),
        LayerKind::Standard => None,
    };
    let Some(root) = root.and_then(|root| fs::canonicalize(root).ok()) else {
        return Ok(None);
    };
    let stem = inputs.project_file.as_deref().and_then(project_stem);
    let Some(relative) = template.relative_path(stem.as_deref()) else {
        return Ok(None);
    };
    let folder = root.join(relative);
    fs::create_dir_all(&folder)
        .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    let folder = fs::canonicalize(&folder)
        .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    if !folder.starts_with(&root) {
        return Err(format!("{} resolves outside its root", folder.display()));
    }
    Ok(Some(folder))
}

/// A folder name for a part, from its id: the standard prefix is dropped and
/// anything but letters, digits, `.`, `_` and `-` becomes `-`.
pub fn folder_name_for(id: &str) -> String {
    let short = id
        .strip_prefix(STANDARD_ID_PREFIX)
        .or_else(|| id.strip_prefix("local."))
        .unwrap_or(id);
    let name = short
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let name = name.trim_matches(['.', '-']).to_owned();
    if name.is_empty() { "part".into() } else { name }
}

/// `parent/base`, or `parent/base-2`, `-3`… when taken.
pub fn unique_folder(parent: &Path, base: &str) -> PathBuf {
    let first = parent.join(base);
    if !first.exists() {
        return first;
    }
    let mut n = 2_u64;
    loop {
        let candidate = parent.join(format!("{base}-{n}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Copies the part `component` (the version currently in use) into `layer`,
/// keeping its id so the copy takes precedence where the layer does.
///
/// # Errors
/// Fails when the layer already defines that id, which would make both
/// copies unloadable, or when the files cannot be written.
pub fn copy_part(component: &ResolvedComponent, layer: &WritableLayer) -> Result<PathBuf, String> {
    let id = &component.definition.id;
    if component.layer == layer.index
        || component
            .shadows
            .iter()
            .any(|shadow| shadow.layer == layer.index)
    {
        return Err(format!("{id} is already in that library"));
    }
    let folder = unique_folder(&layer.path, &folder_name_for(id));
    fs::create_dir(&folder)
        .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    let written = match &component.directory {
        Some(source) => copy_files(source, &folder),
        None => write_files(component, &folder),
    };
    if let Err(error) = written {
        let _ = fs::remove_dir_all(&folder);
        return Err(format!("could not copy {id}: {error}"));
    }
    Ok(folder)
}

fn copy_files(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::copy(source.join(DEFINITION_FILE), target.join(DEFINITION_FILE))?;
    let readme = source.join(DOCUMENTATION_FILE);
    if readme.is_file() {
        fs::copy(readme, target.join(DOCUMENTATION_FILE))?;
    }
    Ok(())
}

fn write_files(component: &ResolvedComponent, target: &Path) -> std::io::Result<()> {
    let documentation = component.documentation.clone().unwrap_or_default();
    let source = ComponentDraft::new(component.definition.clone(), documentation.clone())
        .to_toml()
        .map_err(std::io::Error::other)?;
    fs::write(target.join(DEFINITION_FILE), source)?;
    if !documentation.is_empty() {
        fs::write(target.join(DOCUMENTATION_FILE), documentation)?;
    }
    Ok(())
}

/// A new, empty part folder in `layer`.
///
/// # Errors
/// Fails when the folder cannot be created.
pub fn create_part_folder(layer: &WritableLayer) -> Result<PathBuf, String> {
    let folder = unique_folder(&layer.path, "new-part");
    fs::create_dir(&folder)
        .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use perfboard_library::{ResolveContext, StandardComponentSource, StandardLibrary, resolve};

    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("perfboard-parts-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn definition(id: &str) -> String {
        format!(
            "schema_version = 1\nid = \"{id}\"\ndefinition_version = \"1.0.0\"\nname = \"Part {id}\"\n\
             allowed_rotations = [\"deg0\"]\n[body]\nx = 0.0\ny = 0.0\nwidth = 2.54\nheight = 2.54\n\
             [[pins]]\nid = \"1\"\nnumber = \"1\"\nname = \"Pin\"\nx = 0\ny = 0\n"
        )
    }

    fn write_part(folder: &Path, id: &str) {
        fs::create_dir_all(folder).unwrap();
        fs::write(folder.join(DEFINITION_FILE), definition(id)).unwrap();
    }

    fn standard() -> StandardLibrary {
        StandardLibrary::Embedded {
            version: "1".into(),
            components: vec![StandardComponentSource {
                name: "bridge".into(),
                definition: definition("dev.perfboard.bridge"),
                documentation: Some("# Bridge".into()),
            }],
        }
    }

    fn inputs(
        paths: Option<Vec<String>>,
        file: Option<PathBuf>,
        user: Option<PathBuf>,
    ) -> LibraryInputs {
        LibraryInputs {
            component_paths: paths,
            project_file: file,
            user_components: user,
        }
    }

    fn library(inputs: &LibraryInputs) -> ResolvedLibrary {
        resolve(
            inputs.component_paths.as_deref(),
            &ResolveContext {
                project_file: inputs.project_file.as_deref(),
                user_components: inputs.user_components.as_deref(),
            },
            &standard(),
        )
    }

    #[test]
    fn names_part_folders_after_their_ids() {
        assert_eq!(
            folder_name_for("dev.perfboard.resistor_3_holes"),
            "resistor_3_holes"
        );
        assert_eq!(
            folder_name_for("local.rs485-breakout.2"),
            "rs485-breakout.2"
        );
        assert_eq!(
            folder_name_for("com.example/led strip"),
            "com.example-led-strip"
        );
        assert_eq!(folder_name_for("local..."), "part");
    }

    #[test]
    fn picks_an_unused_folder_name() {
        let dir = TestDir::new("unique");
        assert_eq!(unique_folder(&dir.0, "led"), dir.0.join("led"));
        fs::create_dir(dir.0.join("led")).unwrap();
        fs::create_dir(dir.0.join("led-2")).unwrap();
        assert_eq!(unique_folder(&dir.0, "led"), dir.0.join("led-3"));
    }

    #[test]
    fn lists_the_parts_of_a_library_folder_only() {
        let dir = TestDir::new("folder-parts");
        write_part(&dir.0.join("b"), "local.b");
        write_part(&dir.0.join("a"), "local.a");
        let parts = folder_parts(&dir.0).unwrap();
        assert_eq!(
            parts
                .iter()
                .map(|part| part.id.as_str())
                .collect::<Vec<_>>(),
            ["local.a", "local.b"]
        );
        assert_eq!(parts[0].directory, dir.0.join("a"));
        assert_eq!(parts[0].pin_count, 1);
        // A part folder itself, or an empty folder, is not a library.
        assert_eq!(folder_parts(&dir.0.join("a")), None);
        fs::create_dir(dir.0.join("empty")).unwrap();
        assert_eq!(folder_parts(&dir.0.join("empty")), None);
    }

    #[test]
    fn parts_open_only_from_editable_layers_or_the_picked_folder() {
        let dir = TestDir::new("authorized");
        let project = dir.0.join("board.perfboard.json");
        write_part(&dir.0.join("board.perfboard/led"), "local.led");
        write_part(&dir.0.join("elsewhere/other"), "local.other");
        let inputs = inputs(None, Some(project), None);
        let library = library(&inputs);

        assert_eq!(
            authorized_parent(&library, None, &dir.0.join("board.perfboard/led")),
            Ok(dir.0.join("board.perfboard"))
        );
        assert!(authorized_parent(&library, None, &dir.0.join("elsewhere/other")).is_err());
        assert_eq!(
            authorized_parent(
                &library,
                Some(&dir.0.join("elsewhere")),
                &dir.0.join("elsewhere/other")
            ),
            Ok(dir.0.join("elsewhere"))
        );
        // Only direct children: not the layer itself, nor deeper folders.
        assert!(authorized_parent(&library, None, &dir.0.join("board.perfboard")).is_err());
    }

    #[test]
    fn a_missing_default_project_folder_is_created_on_first_write() {
        let dir = TestDir::new("create-layer");
        let inputs = inputs(None, Some(dir.0.join("board.perfboard.json")), None);
        let library = library(&inputs);
        let layer = writable_layer(&library, &inputs, LayerKind::Project).unwrap();
        assert_eq!(layer.index, 0);
        assert_eq!(layer.path, dir.0.join("board.perfboard"));
        assert!(layer.path.is_dir());
    }

    #[test]
    fn explains_why_no_layer_can_be_written() {
        let unsaved = inputs(None, None, None);
        let error = writable_layer(&library(&unsaved), &unsaved, LayerKind::Project).unwrap_err();
        assert!(error.starts_with("Save the project first"), "{error}");

        let ungranted = inputs(Some(vec!["${USER_COMPONENTS}".into()]), None, None);
        let error = writable_layer(&library(&ungranted), &ungranted, LayerKind::User).unwrap_err();
        assert!(error.starts_with("Grant a user library"), "{error}");

        let none = inputs(Some(vec![]), None, None);
        let error = writable_layer(&library(&none), &none, LayerKind::User).unwrap_err();
        assert!(error.contains("list no user library"), "{error}");

        assert!(writable_layer(&library(&none), &none, LayerKind::Standard).is_err());
    }

    #[test]
    fn copies_a_standard_part_once_and_it_then_takes_precedence() {
        let dir = TestDir::new("copy");
        let inputs = inputs(None, Some(dir.0.join("board.perfboard.json")), None);
        let before = library(&inputs);
        let layer = writable_layer(&before, &inputs, LayerKind::Project).unwrap();
        let bridge = before.get("dev.perfboard.bridge").unwrap();
        let folder = copy_part(bridge, &layer).unwrap();
        assert_eq!(folder, dir.0.join("board.perfboard/bridge"));
        assert_eq!(
            fs::read_to_string(folder.join(DOCUMENTATION_FILE)).unwrap(),
            "# Bridge"
        );

        let after = library(&inputs);
        assert!(after.diagnostics.is_empty(), "{:?}", after.diagnostics);
        let winner = after.get("dev.perfboard.bridge").unwrap();
        assert_eq!(winner.layer, 0);
        assert_eq!(winner.shadows.len(), 1);

        // A second copy into the same library would make both unloadable.
        let layer = writable_layer(&after, &inputs, LayerKind::Project).unwrap();
        assert!(
            copy_part(winner, &layer)
                .unwrap_err()
                .contains("already in that library")
        );
    }

    #[test]
    fn copies_a_part_from_a_folder_verbatim() {
        let dir = TestDir::new("copy-folder");
        let user = dir.0.join("parts");
        write_part(&user.join("led"), "local.led");
        fs::write(user.join("led/README.md"), "# LED").unwrap();
        let inputs = inputs(
            Some(vec![
                "${PROJECT_DIR}/${PROJECT_STEM}.perfboard".into(),
                "${USER_COMPONENTS}".into(),
            ]),
            Some(dir.0.join("board.perfboard.json")),
            Some(user.clone()),
        );
        let library = library(&inputs);
        let layer = writable_layer(&library, &inputs, LayerKind::Project).unwrap();
        let folder = copy_part(library.get("local.led").unwrap(), &layer).unwrap();
        assert_eq!(
            fs::read_to_string(folder.join(DEFINITION_FILE)).unwrap(),
            definition("local.led")
        );
        assert_eq!(
            fs::read_to_string(folder.join(DOCUMENTATION_FILE)).unwrap(),
            "# LED"
        );
    }

    #[test]
    fn new_part_folders_do_not_collide() {
        let dir = TestDir::new("new-part");
        let layer = WritableLayer {
            index: 0,
            path: dir.0.clone(),
        };
        assert_eq!(create_part_folder(&layer).unwrap(), dir.0.join("new-part"));
        assert_eq!(
            create_part_folder(&layer).unwrap(),
            dir.0.join("new-part-2")
        );
    }
}
