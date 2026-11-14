//! Where the standard component library comes from (ADR 0014).
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

/// The library offered for new placements.
///
/// Project layers are not wired in yet, so this is the standard library alone.
/// Diagnostics are logged; the definitions that loaded are still offered.
pub fn current_library() -> ResolvedLibrary {
    let library = resolve(Some(&[]), &ResolveContext::default(), &standard_library());
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

    use perfboard_library::LayerStatus;

    use super::*;

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
