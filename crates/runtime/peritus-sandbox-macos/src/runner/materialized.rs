//! Rollback ownership for target secret files created before native activation completes.

use std::path::{Path, PathBuf};

use crate::{
    MacosError, MacosErrorKind, MacosOperation, PreparationCleanup, RecoveryAction,
};

struct MaterializedSecretFile {
    path: PathBuf,
    created: bool,
}

#[derive(Clone, Copy)]
pub(super) struct MaterializedSecretFileRegistration(usize);

/// Removes every exact file created by the helper if target replacement does not succeed.
pub(super) struct MaterializedSecretFiles {
    files: Vec<MaterializedSecretFile>,
}

impl MaterializedSecretFiles {
    pub(super) const fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub(super) fn register(
        &mut self,
        path: impl AsRef<Path>,
    ) -> MaterializedSecretFileRegistration {
        let registration = MaterializedSecretFileRegistration(self.files.len());
        self.files.push(MaterializedSecretFile {
            path: path.as_ref().to_path_buf(),
            created: false,
        });
        registration
    }

    pub(super) fn created(&mut self, registration: MaterializedSecretFileRegistration) {
        if let Some(file) = self.files.get_mut(registration.0) {
            file.created = true;
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn cancel(&mut self, registration: MaterializedSecretFileRegistration) {
        if registration.0 + 1 == self.files.len()
            && self.files.get(registration.0).is_some_and(|file| !file.created)
        {
            self.files.pop();
        }
    }

    pub(super) fn cleanup(&mut self) -> Result<(), MacosError> {
        let mut failed = false;
        for index in (0..self.files.len()).rev() {
            failed |= self.remove(index).is_err();
        }
        if failed { Err(cleanup_error()) } else { Ok(()) }
    }

    fn remove(&mut self, index: usize) -> Result<(), ()> {
        let Some(file) = self.files.get(index) else {
            return Ok(());
        };
        if !file.created {
            self.files.remove(index);
            return Ok(());
        }
        match std::fs::remove_file(&file.path) {
            Ok(()) => {
                self.files.remove(index);
                Ok(())
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                self.files.remove(index);
                Ok(())
            }
            Err(_) => Err(()),
        }
    }
}

impl Drop for MaterializedSecretFiles {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn cleanup_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::CleanupIncomplete,
        MacosOperation::Activate,
        RecoveryAction::RetryCleanup,
        "materialized secret file cleanup remains incomplete",
    )
    .with_cleanup(PreparationCleanup::new(false, false, true))
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
            let first = files.register(&first);
            files.created(first);
            let second = files.register(&second);
            files.created(second);
        }
        assert!(!first.exists());
        assert!(!second.exists());
    }
}
