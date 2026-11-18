#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Preferences {
    path: PathBuf,
}

impl Preferences {
    #[must_use]
    pub fn load(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the current preference file version.
    ///
    /// # Errors
    ///
    /// Returns an error when the preference file cannot be written.
    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, "workspaces\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_do_not_persist_a_global_source_mode() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("desktop-mode");
        let preferences = Preferences::load(&path);
        preferences.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "workspaces\n");
        assert_eq!(Preferences::load(&path).path(), path);
    }
}
