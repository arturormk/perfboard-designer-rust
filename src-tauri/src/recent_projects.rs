//! Most recently opened or saved project files.
//!
//! The list is local application configuration, not project data: it lives in
//! `recent-projects.json` under the platform configuration directory (for
//! example `~/.config/dev.perfboarddesigner.desktop/` on Linux).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use serde::{Deserialize, Serialize};

/// How many projects the list keeps.
pub const MAX_RECENT_PROJECTS: usize = 10;

const FILE_NAME: &str = "recent-projects.json";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFile {
    version: u32,
    projects: Vec<PathBuf>,
}

/// One entry as shown in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub exists: bool,
}

/// The recent-projects file, serialized through one lock.
pub struct RecentProjects {
    file: Option<PathBuf>,
    lock: Mutex<()>,
}

impl RecentProjects {
    /// Uses `directory/recent-projects.json`. `None` disables persistence,
    /// for when the platform has no configuration directory.
    pub fn new(directory: Option<PathBuf>) -> Self {
        Self {
            file: directory.map(|directory| directory.join(FILE_NAME)),
            lock: Mutex::new(()),
        }
    }

    /// The list, most recent first.
    pub fn list(&self) -> Vec<RecentProject> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.read()
            .iter()
            .map(PathBuf::as_path)
            .map(describe)
            .collect()
    }

    /// Moves `path` to the front of the list.
    pub fn record(&self, path: &Path) -> io::Result<Vec<RecentProject>> {
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.update(|projects| {
            projects.retain(|item| item != &path);
            projects.insert(0, path);
        })
    }

    /// Adds paths at the end of the list, skipping ones it already has. Used
    /// once to adopt the list the webview used to keep in local storage.
    pub fn import(&self, paths: &[PathBuf]) -> io::Result<Vec<RecentProject>> {
        self.update(|projects| {
            for path in paths {
                if !projects.contains(path) {
                    projects.push(path.clone());
                }
            }
        })
    }

    /// Removes `path`, or every entry when `path` is `None`.
    pub fn forget(&self, path: Option<&Path>) -> io::Result<Vec<RecentProject>> {
        self.update(|projects| match path {
            Some(path) => projects.retain(|item| item != path),
            None => projects.clear(),
        })
    }

    fn update(&self, change: impl FnOnce(&mut Vec<PathBuf>)) -> io::Result<Vec<RecentProject>> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut projects = self.read();
        change(&mut projects);
        projects.truncate(MAX_RECENT_PROJECTS);
        self.write(&projects)?;
        Ok(projects
            .iter()
            .map(PathBuf::as_path)
            .map(describe)
            .collect())
    }

    /// A missing or unreadable file is an empty list: losing it costs only
    /// convenience, so it must never stop the app from starting.
    fn read(&self) -> Vec<PathBuf> {
        let Some(file) = &self.file else {
            return Vec::new();
        };
        let Ok(text) = fs::read_to_string(file) else {
            return Vec::new();
        };
        match serde_json::from_str::<RecentFile>(&text) {
            Ok(recent) => recent.projects,
            Err(error) => {
                eprintln!("Ignoring unreadable {}: {error}", file.display());
                Vec::new()
            }
        }
    }

    fn write(&self, projects: &[PathBuf]) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(directory) = file.parent() {
            fs::create_dir_all(directory)?;
        }
        let text = serde_json::to_string_pretty(&RecentFile {
            version: FORMAT_VERSION,
            projects: projects.to_vec(),
        })
        .map_err(io::Error::other)?;
        let partial = file.with_extension("json.partial");
        fs::write(&partial, text + "\n")?;
        fs::rename(&partial, file)
    }
}

fn describe(path: &Path) -> RecentProject {
    RecentProject {
        path: path.to_string_lossy().into_owned(),
        name: path.file_name().map_or_else(
            || path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        ),
        exists: path.is_file(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("perfboard-recent-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn touch(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, "{}").unwrap();
            fs::canonicalize(path).unwrap()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn paths(list: &[RecentProject]) -> Vec<&str> {
        list.iter().map(|item| item.path.as_str()).collect()
    }

    #[test]
    fn record_moves_a_path_to_the_front_and_persists() {
        let dir = TempDir::new("record");
        let a = dir.touch("a.perfboard.json");
        let b = dir.touch("b.perfboard.json");
        let config = dir.0.join("config");
        let recent = RecentProjects::new(Some(config.clone()));
        recent.record(&a).unwrap();
        recent.record(&b).unwrap();
        let list = recent.record(&a).unwrap();
        assert_eq!(
            paths(&list),
            [a.to_str().unwrap(), b.to_str().unwrap()],
            "re-recording moves to the front without duplicating"
        );
        assert_eq!(list[0].name, "a.perfboard.json");
        assert!(list[0].exists);

        let reopened = RecentProjects::new(Some(config));
        assert_eq!(reopened.list(), list);
    }

    #[test]
    fn list_is_capped_and_reports_missing_files() {
        let dir = TempDir::new("cap");
        let recent = RecentProjects::new(Some(dir.0.join("config")));
        for index in 0..MAX_RECENT_PROJECTS + 3 {
            recent.record(&dir.touch(&format!("{index}.json"))).unwrap();
        }
        let list = recent.list();
        assert_eq!(list.len(), MAX_RECENT_PROJECTS);
        assert_eq!(list[0].name, format!("{}.json", MAX_RECENT_PROJECTS + 2));

        fs::remove_file(&list[0].path).unwrap();
        assert!(!recent.list()[0].exists);
    }

    #[test]
    fn forget_removes_one_or_all() {
        let dir = TempDir::new("forget");
        let a = dir.touch("a.json");
        let b = dir.touch("b.json");
        let recent = RecentProjects::new(Some(dir.0.join("config")));
        recent.record(&a).unwrap();
        recent.record(&b).unwrap();
        assert_eq!(
            paths(&recent.forget(Some(&b)).unwrap()),
            [a.to_str().unwrap()]
        );
        assert_eq!(recent.forget(None).unwrap(), []);
    }

    #[test]
    fn import_appends_without_duplicates() {
        let dir = TempDir::new("import");
        let a = dir.touch("a.json");
        let b = dir.touch("b.json");
        let recent = RecentProjects::new(Some(dir.0.join("config")));
        recent.record(&a).unwrap();
        let list = recent.import(&[b.clone(), a.clone()]).unwrap();
        assert_eq!(paths(&list), [a.to_str().unwrap(), b.to_str().unwrap()]);
    }

    #[test]
    fn corrupt_or_absent_configuration_is_an_empty_list() {
        let dir = TempDir::new("corrupt");
        assert_eq!(RecentProjects::new(None).list(), []);
        let config = dir.0.join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join(FILE_NAME), "not json").unwrap();
        let recent = RecentProjects::new(Some(config));
        assert_eq!(recent.list(), []);
        let a = dir.touch("a.json");
        assert_eq!(recent.record(&a).unwrap().len(), 1, "rewrites a bad file");
    }
}
