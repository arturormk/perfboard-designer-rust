use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

fn main() {
    perfboard_version::emit_build_env();
    embed_standard_components();
    tauri_build::build();
}

/// Writes `$OUT_DIR/standard_components.rs`: one `EmbeddedSource` per
/// `components/<name>/component.toml`, with its `README.md` when present
/// (ADR 0014). Hidden folders and anything else in a component folder, such
/// as the editor's `*.bak` backups, are left out.
fn embed_standard_components() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("called outside a build script"));
    let root = manifest_dir.join("../components");
    // Cargo scans a directory given here for changes anywhere below it.
    println!("cargo:rerun-if-changed={}", root.display());

    let mut folders = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()))
        .map(|entry| entry.expect("readable component folder").path())
        .filter(|path| {
            path.join("component.toml").is_file()
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
        })
        .collect::<Vec<_>>();
    folders.sort();

    let mut code = String::from("&[\n");
    for folder in &folders {
        let name = folder
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("component folder {} is not UTF-8", folder.display()));
        let definition = canonical(&folder.join("component.toml"));
        let readme = folder.join("README.md");
        let documentation = if readme.is_file() {
            format!("Some(include_str!({:?}))", canonical(&readme))
        } else {
            "None".to_owned()
        };
        writeln!(
            code,
            "    EmbeddedSource {{ name: {name:?}, definition: include_str!({definition:?}), documentation: {documentation} }},"
        )
        .expect("writing to a String cannot fail");
    }
    code.push(']');

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("called outside a build script"));
    fs::write(out.join("standard_components.rs"), code)
        .expect("cannot write standard_components.rs");
}

fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("cannot resolve {}: {error}", path.display()))
        .to_str()
        .unwrap_or_else(|| panic!("{} is not UTF-8", path.display()))
        .to_owned()
}
