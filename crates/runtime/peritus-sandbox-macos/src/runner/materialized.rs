//! Rollback ownership for target secret files created before native activation completes.

use std::path::{Path, PathBuf};

/// Removes every exact file created by the helper if target replacement does not succeed.
pub(super) struct MaterializedSecretFiles {
    paths: Vec<PathBuf>,
}

impl MaterializedSecretFiles {
    pub(super) const fn new() -> Self {
        Self { paths: Vec::new() }
    }

    pub(super) fn record(&mut self, path: impl AsRef<Path>) {
        self.paths.push(path.as_ref().to_path_buf());
    }
}

impl Drop for MaterializedSecretFiles {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MaterializedSecretFiles;

    #[test]
    fn failed_activation_rolls_back_every_materialized_file() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.secret");
        let second = directory.path().join("second.secret");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();
        {
            let mut files = MaterializedSecretFiles::new();
            files.record(&first);
            files.record(&second);
        }
        assert!(!first.exists());
        assert!(!second.exists());
    }
}
