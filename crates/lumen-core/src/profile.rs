//! Profiles are scenes with a name, stored as JSON files in one directory.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scene::Scene;
use crate::{Error, Result};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub scene: Scene,
}

#[derive(Clone, Debug)]
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Profile names, sorted, read from the files' contents (not the file names).
    pub fn list(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(names),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|x| x == "json") {
                match fs::read_to_string(&path).map_err(Error::from).and_then(|s| Ok(serde_json::from_str::<Profile>(&s)?)) {
                    Ok(p) => names.push(p.name),
                    Err(e) => tracing::warn!(?path, %e, "skipping unreadable profile"),
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn load(&self, name: &str) -> Result<Profile> {
        let path = self.path_for(name);
        let text = fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::ProfileNotFound(name.to_string()),
            _ => Error::Io(e),
        })?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Write atomically (temp file + rename) so a crash never leaves a half-written profile.
    pub fn save(&self, profile: &Profile) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path_for(&profile.name);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(profile)?)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        match fs::remove_file(self.path_for(name)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::ProfileNotFound(name.to_string())),
            Err(e) => Err(e.into()),
        }
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.json", slug(name)))
    }
}

/// File-system-safe, case-insensitive-unique version of a profile name.
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() { "profile".to_string() } else { trimmed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Night Mode!"), "night-mode");
        assert_eq!(slug("  ...  "), "profile");
        assert_eq!(slug("Übung 2"), "übung-2");
    }

    #[test]
    fn save_list_load_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::new(dir.path().join("profiles"));
        assert!(store.list().unwrap().is_empty(), "missing dir is just empty");

        let p = Profile { name: "Work Day".into(), scene: Scene::default() };
        store.save(&p).unwrap();
        assert_eq!(store.list().unwrap(), vec!["Work Day"]);
        assert_eq!(store.load("Work Day").unwrap(), p);
        assert!(dir.path().join("profiles/work-day.json").exists());

        store.delete("Work Day").unwrap();
        assert!(matches!(store.load("Work Day"), Err(Error::ProfileNotFound(_))));
        assert!(matches!(store.delete("Work Day"), Err(Error::ProfileNotFound(_))));
    }
}
