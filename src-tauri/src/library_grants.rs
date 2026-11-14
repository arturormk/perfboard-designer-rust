//! User-library grants (ADR 0014): which folder `${USER_COMPONENTS}` means
//! for a project on this machine.
//!
//! A grant is local configuration, never project data, so a project that is
//! copied or shared carries none: its recipient sees only the project's own
//! parts and the standard library until they grant access themselves. Each
//! grant is `library-grants/<project_id>.toml` under the platform
//! configuration directory (for example
//! `~/.config/dev.perfboarddesigner.desktop/` on Linux).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use perfboard_core::ProjectId;
use serde::{Deserialize, Serialize};

const DIRECTORY: &str = "library-grants";

/// One project's grant as stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryGrant {
    /// The folder `${USER_COMPONENTS}` stands for.
    pub user_components: PathBuf,
    /// For display in Preferences only.
    #[serde(default)]
    pub project_name: String,
    /// For display in Preferences only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_path: Option<PathBuf>,
}

/// A grant as listed in Preferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GrantEntry {
    pub project_id: String,
    #[serde(flatten)]
    pub grant: LibraryGrant,
    /// Whether the granted folder still exists.
    pub exists: bool,
}

pub struct LibraryGrants {
    directory: Option<PathBuf>,
    lock: Mutex<()>,
}

impl LibraryGrants {
    /// Keeps grants under `config/library-grants/`; `None` keeps nothing, so
    /// no project has a user library.
    pub fn new(config: Option<PathBuf>) -> Self {
        Self {
            directory: config.map(|config| config.join(DIRECTORY)),
            lock: Mutex::new(()),
        }
    }

    fn file(&self, project_id: ProjectId) -> Option<PathBuf> {
        self.directory
            .as_ref()
            .map(|directory| directory.join(format!("{project_id}.toml")))
    }

    /// The project's grant. A missing or unreadable file is no grant.
    pub fn get(&self, project_id: ProjectId) -> Option<LibraryGrant> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        read(&self.file(project_id)?)
    }

    /// Grants `folder` to the project, replacing any earlier grant.
    ///
    /// # Errors
    /// Fails when the folder is not an existing directory or the grant cannot
    /// be written.
    pub fn grant(
        &self,
        project_id: ProjectId,
        folder: &Path,
        project_name: &str,
        project_file: Option<&Path>,
    ) -> io::Result<LibraryGrant> {
        let folder = fs::canonicalize(folder)?;
        if !folder.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is not a folder", folder.display()),
            ));
        }
        let grant = LibraryGrant {
            user_components: folder,
            project_name: project_name.to_owned(),
            last_seen_path: project_file.map(Path::to_path_buf),
        };
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = self.file(project_id) else {
            return Err(io::Error::other(
                "this system has no configuration folder to keep grants in",
            ));
        };
        write(&file, &grant)?;
        Ok(grant)
    }

    /// Removes the project's grant, if it has one.
    ///
    /// # Errors
    /// Fails when the grant file exists but cannot be removed.
    pub fn revoke(&self, project_id: ProjectId) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = self.file(project_id) else {
            return Ok(());
        };
        match fs::remove_file(file) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    /// Refreshes the display fields of an existing grant after the project
    /// was opened or saved. Never fails the open or save: errors are logged.
    pub fn touch(&self, project_id: ProjectId, project_name: &str, project_file: &Path) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = self.file(project_id) else {
            return;
        };
        let Some(mut grant) = read(&file) else {
            return;
        };
        let path = fs::canonicalize(project_file).unwrap_or_else(|_| project_file.to_path_buf());
        if grant.project_name == project_name && grant.last_seen_path.as_ref() == Some(&path) {
            return;
        }
        project_name.clone_into(&mut grant.project_name);
        grant.last_seen_path = Some(path);
        if let Err(error) = write(&file, &grant) {
            eprintln!("Could not update {}: {error}", file.display());
        }
    }

    /// Every grant, by project name.
    pub fn list(&self) -> Vec<GrantEntry> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(directory) = &self.directory else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut grants = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let project_id = path
                    .file_name()?
                    .to_str()?
                    .strip_suffix(".toml")?
                    .parse::<uuid::Uuid>()
                    .ok()?
                    .to_string();
                let grant = read(&path)?;
                Some(GrantEntry {
                    project_id,
                    exists: grant.user_components.is_dir(),
                    grant,
                })
            })
            .collect::<Vec<_>>();
        grants.sort_by(|left, right| {
            left.grant
                .project_name
                .to_lowercase()
                .cmp(&right.grant.project_name.to_lowercase())
                .then_with(|| left.project_id.cmp(&right.project_id))
        });
        grants
    }
}

fn read(file: &Path) -> Option<LibraryGrant> {
    let text = fs::read_to_string(file).ok()?;
    toml::from_str(&text)
        .map_err(|error| eprintln!("Ignoring unreadable {}: {error}", file.display()))
        .ok()
}

fn write(file: &Path, grant: &LibraryGrant) -> io::Result<()> {
    if let Some(directory) = file.parent() {
        fs::create_dir_all(directory)?;
    }
    let text = toml::to_string(grant).map_err(io::Error::other)?;
    let partial = file.with_extension("toml.partial");
    fs::write(&partial, text)?;
    fs::rename(&partial, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("perfboard-grants-{name}-{}", std::process::id()));
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

    #[test]
    fn grants_are_kept_per_project_and_survive_a_restart() {
        let dir = TestDir::new("roundtrip");
        let parts = dir.0.join("parts");
        fs::create_dir_all(&parts).unwrap();
        let (first, second) = (ProjectId::new(), ProjectId::new());
        let grants = LibraryGrants::new(Some(dir.0.join("config")));
        assert_eq!(grants.get(first), None);

        let grant = grants
            .grant(
                first,
                &parts,
                "Board",
                Some(Path::new("/w/board.perfboard.json")),
            )
            .unwrap();
        assert_eq!(grant.user_components, parts);

        let reopened = LibraryGrants::new(Some(dir.0.join("config")));
        assert_eq!(reopened.get(first), Some(grant));
        assert_eq!(reopened.get(second), None);
        assert!(
            dir.0
                .join(format!("config/library-grants/{first}.toml"))
                .is_file()
        );
    }

    #[test]
    fn only_existing_folders_can_be_granted() {
        let dir = TestDir::new("missing");
        let grants = LibraryGrants::new(Some(dir.0.join("config")));
        let project = ProjectId::new();
        assert!(
            grants
                .grant(project, &dir.0.join("nope"), "P", None)
                .is_err()
        );
        let file = dir.0.join("file.txt");
        fs::write(&file, "x").unwrap();
        assert!(grants.grant(project, &file, "P", None).is_err());
        assert_eq!(grants.get(project), None);
    }

    #[test]
    fn revoking_removes_the_grant_and_is_idempotent() {
        let dir = TestDir::new("revoke");
        let grants = LibraryGrants::new(Some(dir.0.join("config")));
        let project = ProjectId::new();
        grants.grant(project, &dir.0, "P", None).unwrap();
        grants.revoke(project).unwrap();
        assert_eq!(grants.get(project), None);
        grants.revoke(project).unwrap();
    }

    #[test]
    fn touch_updates_display_fields_of_existing_grants_only() {
        let dir = TestDir::new("touch");
        let grants = LibraryGrants::new(Some(dir.0.join("config")));
        let (granted, other) = (ProjectId::new(), ProjectId::new());
        grants.grant(granted, &dir.0, "Old", None).unwrap();
        grants.touch(granted, "New", Path::new("/w/new.perfboard.json"));
        grants.touch(other, "Other", Path::new("/w/other.perfboard.json"));
        let grant = grants.get(granted).unwrap();
        assert_eq!(grant.project_name, "New");
        assert_eq!(
            grant.last_seen_path,
            Some(PathBuf::from("/w/new.perfboard.json"))
        );
        assert_eq!(grant.user_components, dir.0);
        assert_eq!(grants.get(other), None);
    }

    #[test]
    fn lists_grants_by_name_and_skips_stray_files() {
        let dir = TestDir::new("list");
        let parts = dir.0.join("parts");
        fs::create_dir_all(&parts).unwrap();
        let grants = LibraryGrants::new(Some(dir.0.join("config")));
        let (zeta, alpha, gone) = (ProjectId::new(), ProjectId::new(), ProjectId::new());
        grants.grant(zeta, &parts, "zeta", None).unwrap();
        grants.grant(alpha, &parts, "Alpha", None).unwrap();
        let removed = dir.0.join("removed");
        fs::create_dir_all(&removed).unwrap();
        grants.grant(gone, &removed, "Gone", None).unwrap();
        fs::remove_dir(&removed).unwrap();
        let folder = dir.0.join("config/library-grants");
        fs::write(folder.join("notes.toml"), "x = 1").unwrap();
        fs::write(
            folder.join(format!("{}.toml", ProjectId::new())),
            "not toml [",
        )
        .unwrap();

        let listed = grants.list();
        assert_eq!(
            listed
                .iter()
                .map(|entry| (entry.grant.project_name.as_str(), entry.exists))
                .collect::<Vec<_>>(),
            [("Alpha", true), ("Gone", false), ("zeta", true)]
        );
        assert_eq!(listed[0].project_id, alpha.to_string());
    }

    #[test]
    fn without_a_configuration_folder_nothing_is_granted() {
        let grants = LibraryGrants::new(None);
        let project = ProjectId::new();
        assert!(grants.grant(project, Path::new("/"), "P", None).is_err());
        assert_eq!(grants.get(project), None);
        assert_eq!(grants.list(), []);
        grants.revoke(project).unwrap();
    }
}
