//! Version resolution for Perfboard Designer.
//!
//! The workspace version in the root `Cargo.toml` is the single source of
//! truth, interpreted by the parity of its minor number (see ADR 0010):
//!
//! - **Odd minor** (`0.1.0`, `0.3.0`, …) is a development line. The patch
//!   number in `Cargo.toml` must stay `0`; the effective patch is the number of
//!   commits reachable from `HEAD`, so every commit has a distinct, increasing
//!   version (`0.1.26`, `0.1.27`, …).
//! - **Even minor** (`0.2.0`, `0.2.1`, …) is a release line. The version is
//!   used exactly as written and only ever changes by hand.
//!
//! Build scripts call [`emit_build_env`], which exposes the effective version
//! to the crate as `env!("PERFBOARD_VERSION")`. The `perfboard-version` binary
//! prints the same value for scripts and CI.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable that supplies the commit count when the source tree
/// has no Git metadata (for example, a source archive).
pub const COMMIT_COUNT_ENV: &str = "PERFBOARD_COMMIT_COUNT";

/// A `major.minor.patch` version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parses a plain `major.minor.patch` version without pre-release or build
    /// metadata.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::Malformed`] when `text` is not three
    /// dot-separated decimal numbers.
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        let malformed = || VersionError::Malformed(text.to_owned());
        let mut parts = text.split('.').map(|part| {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed());
            }
            part.parse::<u64>().map_err(|_| malformed())
        });
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(malformed());
        };
        Ok(Self {
            major: major?,
            minor: minor?,
            patch: patch?,
        })
    }

    /// Whether this version belongs to a manually versioned release line.
    #[must_use]
    pub fn is_release_line(self) -> bool {
        self.minor.is_multiple_of(2)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Why a version could not be resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionError {
    /// The text is not a plain `major.minor.patch` version.
    Malformed(String),
    /// A development line (odd minor) declared a patch number other than 0.
    ManualPatchOnDevelopmentLine(Version),
    /// A development line was built without Git metadata or an explicit count.
    MissingCommitCount(Version),
}

impl fmt::Display for VersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(text) => {
                write!(
                    formatter,
                    "`{text}` is not a plain major.minor.patch version"
                )
            }
            Self::ManualPatchOnDevelopmentLine(version) => write!(
                formatter,
                "{version} is on a development line (odd minor), so its patch number is the \
                 commit count; set the workspace version to {}.{}.0",
                version.major, version.minor
            ),
            Self::MissingCommitCount(version) => write!(
                formatter,
                "{version} is on a development line (odd minor) and needs the commit count; \
                 build from a Git checkout with full history or set {COMMIT_COUNT_ENV}"
            ),
        }
    }
}

impl std::error::Error for VersionError {}

/// Resolves the effective version from the declared workspace version.
///
/// # Errors
///
/// Fails when `declared` is malformed, when a development line declares a
/// patch number, or when a development line has no commit count.
pub fn resolve(declared: &str, commit_count: Option<u64>) -> Result<Version, VersionError> {
    let version = Version::parse(declared)?;
    if version.is_release_line() {
        return Ok(version);
    }
    if version.patch != 0 {
        return Err(VersionError::ManualPatchOnDevelopmentLine(version));
    }
    let patch = commit_count.ok_or(VersionError::MissingCommitCount(version))?;
    Ok(Version { patch, ..version })
}

/// Counts the commits reachable from `HEAD` in the repository containing
/// `directory`, honoring the [`COMMIT_COUNT_ENV`] override.
#[must_use]
pub fn commit_count(directory: &Path) -> Option<u64> {
    if let Ok(value) = std::env::var(COMMIT_COUNT_ENV) {
        return value.trim().parse().ok();
    }
    git(directory, &["rev-list", "--count", "HEAD"])?
        .parse()
        .ok()
}

/// Build-script entry point: resolves the version declared by the calling
/// package and exposes it as the `PERFBOARD_VERSION` compile-time variable.
///
/// It also registers the Git files that move when a commit is made, so the
/// version is recomputed after each commit without rebuilding otherwise.
///
/// # Panics
///
/// Panics, failing the build with an explanation, when the version cannot be
/// resolved. Must only be called from a build script.
pub fn emit_build_env() {
    let declared = std::env::var("CARGO_PKG_VERSION").expect("called outside a build script");
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("called outside a build script"),
    );
    println!("cargo:rerun-if-env-changed={COMMIT_COUNT_ENV}");
    for variable in ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    for path in head_tracking_files(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    match resolve(&declared, commit_count(&manifest_dir)) {
        Ok(version) => println!("cargo:rustc-env=PERFBOARD_VERSION={version}"),
        Err(error) => panic!("cannot determine the Perfboard Designer version: {error}"),
    }
}

/// The existing Git files whose modification means `HEAD` moved.
fn head_tracking_files(directory: &Path) -> Vec<PathBuf> {
    let Some(git_dir) = git(directory, &["rev-parse", "--absolute-git-dir"]) else {
        return Vec::new();
    };
    let git_dir = PathBuf::from(git_dir);
    let common_dir = git(directory, &["rev-parse", "--git-common-dir"])
        .map_or_else(|| git_dir.clone(), |dir| directory.join(dir));
    let mut files = vec![git_dir.join("HEAD"), common_dir.join("packed-refs")];
    if let Some(reference) = git(directory, &["symbolic-ref", "-q", "HEAD"]) {
        files.push(git_dir.join(&reference));
        files.push(common_dir.join(&reference));
    }
    // A missing file would make Cargo rerun the build script on every build.
    files.retain(|path| path.is_file());
    files.dedup();
    files
}

fn git(directory: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_lines_take_the_commit_count_as_patch() {
        assert_eq!(resolve("0.1.0", Some(26)).unwrap().to_string(), "0.1.26");
        assert_eq!(resolve("1.3.0", Some(1)).unwrap().to_string(), "1.3.1");
    }

    #[test]
    fn release_lines_are_used_verbatim() {
        assert_eq!(resolve("0.2.0", Some(40)).unwrap().to_string(), "0.2.0");
        assert_eq!(resolve("0.2.3", None).unwrap().to_string(), "0.2.3");
        assert_eq!(resolve("1.0.0", None).unwrap().to_string(), "1.0.0");
    }

    #[test]
    fn development_lines_reject_a_manual_patch() {
        assert!(matches!(
            resolve("0.1.4", Some(26)),
            Err(VersionError::ManualPatchOnDevelopmentLine(_))
        ));
    }

    #[test]
    fn development_lines_need_a_commit_count() {
        let error = resolve("0.1.0", None).unwrap_err();
        assert!(matches!(error, VersionError::MissingCommitCount(_)));
        assert!(error.to_string().contains(COMMIT_COUNT_ENV));
    }

    #[test]
    fn only_plain_versions_parse() {
        for text in [
            "0.1",
            "0.1.0.0",
            "0.1.0-beta",
            "v0.1.0",
            "0..1",
            "a.b.c",
            "0.1.+1",
        ] {
            assert!(Version::parse(text).is_err(), "{text} should be rejected");
        }
    }

    fn read_json(relative: &str) -> serde_json::Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative);
        let text = std::fs::read_to_string(&path).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn frontend_package_declares_the_workspace_version() {
        assert_eq!(
            read_json("frontend/package.json")["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn tauri_bundle_version_comes_from_cargo() {
        // Without a `version` key, Tauri uses the Cargo package version, so
        // release bundles carry the manually set release-line version.
        assert!(
            read_json("src-tauri/tauri.conf.json")
                .get("version")
                .is_none()
        );
    }
}
