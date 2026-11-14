//! Parsing of `component_paths` entries (ADR 0014).

use std::{fmt, path::PathBuf, str::FromStr};

use serde::Serialize;
use thiserror::Error;

pub const PROJECT_DIR: &str = "${PROJECT_DIR}";
pub const USER_COMPONENTS: &str = "${USER_COMPONENTS}";
pub const PROJECT_STEM: &str = "${PROJECT_STEM}";

/// The layer used when a project has no `component_paths` field.
pub const DEFAULT_PROJECT_LAYER: &str = "${PROJECT_DIR}/${PROJECT_STEM}.perfboard";

/// The folder a layer path is anchored to and confined within.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerRoot {
    /// The folder holding the project file.
    ProjectDir,
    /// The user library granted to this project.
    UserComponents,
}

impl LayerRoot {
    #[must_use]
    pub const fn variable(self) -> &'static str {
        match self {
            Self::ProjectDir => PROJECT_DIR,
            Self::UserComponents => USER_COMPONENTS,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum PathTemplateError {
    #[error("a component path must start with ${{PROJECT_DIR}} or ${{USER_COMPONENTS}}")]
    MissingRoot,
    #[error("component paths use `/` as separator, not `\\`")]
    Backslash,
    #[error("component paths cannot contain empty segments")]
    EmptySegment,
    #[error("component paths cannot contain `.` or `..` segments")]
    RelativeSegment,
    #[error(
        "unsupported variable or `$` in segment `{0}`; only ${{PROJECT_STEM}} may follow the root"
    )]
    UnsupportedVariable(String),
    #[error("component paths cannot contain control characters")]
    ControlCharacter,
}

/// A validated `component_paths` entry: a root plus relative segments, each of
/// which may contain `${PROJECT_STEM}`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PathTemplate {
    root: LayerRoot,
    segments: Vec<String>,
}

impl PathTemplate {
    /// Parses and validates a template.
    ///
    /// # Errors
    /// Returns the first rule the template breaks.
    pub fn parse(source: &str) -> Result<Self, PathTemplateError> {
        if source.chars().any(char::is_control) {
            return Err(PathTemplateError::ControlCharacter);
        }
        if source.contains('\\') {
            return Err(PathTemplateError::Backslash);
        }
        let (root, rest) = [LayerRoot::ProjectDir, LayerRoot::UserComponents]
            .into_iter()
            .find_map(|root| {
                source
                    .strip_prefix(root.variable())
                    .map(|rest| (root, rest))
            })
            .ok_or(PathTemplateError::MissingRoot)?;
        let segments = if rest.is_empty() {
            Vec::new()
        } else {
            let rest = rest
                .strip_prefix('/')
                .ok_or(PathTemplateError::MissingRoot)?;
            rest.split('/')
                .map(validate_segment)
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(Self { root, segments })
    }

    #[must_use]
    pub const fn root(&self) -> LayerRoot {
        self.root
    }

    /// The path below the root, with `${PROJECT_STEM}` replaced by `stem`.
    ///
    /// Returns `None` when the template uses `${PROJECT_STEM}` and no stem is
    /// known, or when substitution produces a `.`/`..` segment or a separator.
    #[must_use]
    pub fn relative_path(&self, stem: Option<&str>) -> Option<PathBuf> {
        let mut path = PathBuf::new();
        for segment in &self.segments {
            let segment = if segment.contains(PROJECT_STEM) {
                segment.replace(PROJECT_STEM, stem?)
            } else {
                segment.clone()
            };
            if segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.contains(['/', '\\'])
            {
                return None;
            }
            path.push(segment);
        }
        Some(path)
    }
}

fn validate_segment(segment: &str) -> Result<String, PathTemplateError> {
    if segment.is_empty() {
        return Err(PathTemplateError::EmptySegment);
    }
    if segment == "." || segment == ".." {
        return Err(PathTemplateError::RelativeSegment);
    }
    if segment.replace(PROJECT_STEM, "").contains('$') {
        return Err(PathTemplateError::UnsupportedVariable(segment.to_owned()));
    }
    Ok(segment.to_owned())
}

impl FromStr for PathTemplate {
    type Err = PathTemplateError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Self::parse(source)
    }
}

impl fmt::Display for PathTemplate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.root.variable())?;
        for segment in &self.segments {
            write!(formatter, "/{segment}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_both_roots_with_and_without_segments() {
        for source in [
            "${PROJECT_DIR}",
            "${USER_COMPONENTS}",
            "${PROJECT_DIR}/parts",
            "${USER_COMPONENTS}/rs485/v2",
            DEFAULT_PROJECT_LAYER,
            "${PROJECT_DIR}/${PROJECT_STEM}-parts/${PROJECT_STEM}",
        ] {
            let template = PathTemplate::parse(source).unwrap();
            assert_eq!(template.to_string(), source);
        }
        assert_eq!(
            PathTemplate::parse("${USER_COMPONENTS}/x").unwrap().root(),
            LayerRoot::UserComponents
        );
    }

    #[test]
    fn requires_a_root() {
        for source in [
            "",
            "parts",
            "/home/user/parts",
            "~/parts",
            "~",
            "${HOME}/parts",
            "${PROJECT_STEM}/parts",
            "${PROJECT_DIRS}/parts",
            "${PROJECT_DIR}parts",
            "x${PROJECT_DIR}/parts",
            "C:/parts",
        ] {
            assert_eq!(
                PathTemplate::parse(source),
                Err(PathTemplateError::MissingRoot),
                "{source}"
            );
        }
    }

    #[test]
    fn rejects_relative_and_empty_segments() {
        for source in [
            "${PROJECT_DIR}/..",
            "${PROJECT_DIR}/parts/../..",
            "${USER_COMPONENTS}/./parts",
        ] {
            assert_eq!(
                PathTemplate::parse(source),
                Err(PathTemplateError::RelativeSegment),
                "{source}"
            );
        }
        for source in [
            "${PROJECT_DIR}/",
            "${PROJECT_DIR}//parts",
            "${USER_COMPONENTS}/parts/",
        ] {
            assert_eq!(
                PathTemplate::parse(source),
                Err(PathTemplateError::EmptySegment),
                "{source}"
            );
        }
    }

    #[test]
    fn rejects_other_variables_backslashes_and_control_characters() {
        assert!(matches!(
            PathTemplate::parse("${PROJECT_DIR}/${HOME}"),
            Err(PathTemplateError::UnsupportedVariable(_))
        ));
        assert!(matches!(
            PathTemplate::parse("${PROJECT_DIR}/$PROJECT_STEM"),
            Err(PathTemplateError::UnsupportedVariable(_))
        ));
        assert!(matches!(
            PathTemplate::parse("${PROJECT_DIR}/${USER_COMPONENTS}"),
            Err(PathTemplateError::UnsupportedVariable(_))
        ));
        assert_eq!(
            PathTemplate::parse("${PROJECT_DIR}\\..\\x"),
            Err(PathTemplateError::Backslash)
        );
        assert_eq!(
            PathTemplate::parse("${PROJECT_DIR}/a\nb"),
            Err(PathTemplateError::ControlCharacter)
        );
    }

    #[test]
    fn substitutes_the_project_stem() {
        let template = PathTemplate::parse(DEFAULT_PROJECT_LAYER).unwrap();
        assert_eq!(
            template.relative_path(Some("nano-rs485")),
            Some(PathBuf::from("nano-rs485.perfboard"))
        );
        assert_eq!(template.relative_path(None), None);
        assert_eq!(
            PathTemplate::parse("${PROJECT_DIR}/parts")
                .unwrap()
                .relative_path(None),
            Some(PathBuf::from("parts"))
        );
    }

    #[test]
    fn a_stem_cannot_smuggle_in_relative_segments() {
        let template = PathTemplate::parse("${PROJECT_DIR}/${PROJECT_STEM}").unwrap();
        for stem in ["..", ".", "", "a/b", "a\\b"] {
            assert_eq!(template.relative_path(Some(stem)), None, "{stem:?}");
        }
    }
}
