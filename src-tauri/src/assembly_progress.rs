//! Assembly checkmarks: which plan steps the builder has done.
//!
//! Progress is about the physical board on the bench, not the design, so it is
//! kept out of the project file and its revision history (blueprint §15.4). It
//! lives in `assembly-progress.json` in the platform configuration directory,
//! keyed by project ID, then step ID. Each checkmark records the step's
//! fingerprint when it was ticked: if the step later asks for something
//! different, the mark no longer matches and the UI shows it as changed rather
//! than done.

use std::{
    collections::BTreeMap,
    fs, io,
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "assembly-progress.json";
const FORMAT_VERSION: u32 = 1;

/// Step ID → fingerprint of the step when it was marked done.
pub type Checkmarks = BTreeMap<String, String>;

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProgressFile {
    version: u32,
    projects: BTreeMap<String, Checkmarks>,
}

pub struct AssemblyProgress {
    file: Option<PathBuf>,
    lock: Mutex<()>,
}

impl AssemblyProgress {
    /// Uses `directory/assembly-progress.json`; `None` keeps nothing on disk.
    pub fn new(directory: Option<PathBuf>) -> Self {
        Self {
            file: directory.map(|directory| directory.join(FILE_NAME)),
            lock: Mutex::new(()),
        }
    }

    pub fn get(&self, project_id: &str) -> Checkmarks {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.read().projects.remove(project_id).unwrap_or_default()
    }

    /// Marks a step done with its current fingerprint, or clears it.
    pub fn set(
        &self,
        project_id: &str,
        step_id: &str,
        fingerprint: Option<&str>,
    ) -> io::Result<Checkmarks> {
        self.update(project_id, |marks| match fingerprint {
            Some(fingerprint) => {
                marks.insert(step_id.to_owned(), fingerprint.to_owned());
            }
            None => {
                marks.remove(step_id);
            }
        })
    }

    /// Forgets every checkmark of a project.
    pub fn clear(&self, project_id: &str) -> io::Result<Checkmarks> {
        self.update(project_id, BTreeMap::clear)
    }

    fn update(
        &self,
        project_id: &str,
        change: impl FnOnce(&mut Checkmarks),
    ) -> io::Result<Checkmarks> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = self.read();
        let marks = file.projects.entry(project_id.to_owned()).or_default();
        change(marks);
        let result = marks.clone();
        if result.is_empty() {
            file.projects.remove(project_id);
        }
        self.write(&file)?;
        Ok(result)
    }

    /// A missing or unreadable file is no progress: never block the app.
    fn read(&self) -> ProgressFile {
        let Some(file) = &self.file else {
            return ProgressFile::default();
        };
        let Ok(text) = fs::read_to_string(file) else {
            return ProgressFile::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|error| {
            eprintln!("Ignoring unreadable {}: {error}", file.display());
            ProgressFile::default()
        })
    }

    fn write(&self, progress: &ProgressFile) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(directory) = file.parent() {
            fs::create_dir_all(directory)?;
        }
        let text = serde_json::to_string_pretty(&ProgressFile {
            version: FORMAT_VERSION,
            projects: progress.projects.clone(),
        })
        .map_err(io::Error::other)?;
        let partial = file.with_extension("json.partial");
        fs::write(&partial, text + "\n")?;
        fs::rename(&partial, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("perfboard-progress-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn checkmarks_persist_per_project() {
        let dir = directory("persist");
        let progress = AssemblyProgress::new(Some(dir.clone()));
        progress.set("p1", "wire:a", Some("f1")).unwrap();
        progress.set("p1", "component:b", Some("f2")).unwrap();
        progress.set("p2", "wire:a", Some("f9")).unwrap();
        let reopened = AssemblyProgress::new(Some(dir.clone()));
        assert_eq!(reopened.get("p1").len(), 2);
        assert_eq!(reopened.get("p2")["wire:a"], "f9");
        assert_eq!(reopened.get("p3"), Checkmarks::new());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unticking_and_clearing_remove_marks() {
        let dir = directory("clear");
        let progress = AssemblyProgress::new(Some(dir.clone()));
        progress.set("p1", "wire:a", Some("f1")).unwrap();
        progress.set("p1", "wire:b", Some("f2")).unwrap();
        assert_eq!(progress.set("p1", "wire:a", None).unwrap().len(), 1);
        assert_eq!(progress.clear("p1").unwrap(), Checkmarks::new());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupt_file_is_no_progress() {
        let dir = directory("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE_NAME), "{").unwrap();
        let progress = AssemblyProgress::new(Some(dir.clone()));
        assert_eq!(progress.get("p1"), Checkmarks::new());
        assert_eq!(progress.set("p1", "wire:a", Some("f")).unwrap().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
}
