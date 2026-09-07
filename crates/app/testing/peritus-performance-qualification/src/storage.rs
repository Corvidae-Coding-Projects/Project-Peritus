//! Explicit subject placement and observed filesystem identity.

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use peritus_benchmarks::StableId;
use serde::Serialize;
use tempfile::{Builder, TempDir};

use crate::SubjectError;

/// Exact executable and operator-reviewed filesystem used by every disposable subject.
#[derive(Clone, Debug)]
pub struct SubjectConfiguration {
    executable: PathBuf,
    scratch: StorageObservation,
}

impl SubjectConfiguration {
    /// Resolves an existing executable and scratch directory without an ambient temporary root.
    ///
    /// Storage generation remains an independently reviewed operator fact. The scratch path must
    /// reside on that storage, and must be short enough for the daemon's native Unix socket.
    ///
    /// # Errors
    ///
    /// Rejects missing paths, a non-file executable, or a non-directory scratch root.
    pub fn new(executable: &Path, scratch_root: &Path) -> Result<Self, SubjectError> {
        let executable = fs::canonicalize(executable)?;
        if !fs::metadata(&executable)?.is_file() {
            return Err(SubjectError::Configuration("daemon must be a regular file".to_owned()));
        }
        Ok(Self { executable, scratch: StorageObservation::observe(scratch_root)? })
    }

    /// Returns the exact canonical executable selected for the campaign.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the actual directory and filesystem identity that the operator selected.
    #[must_use]
    pub const fn scratch(&self) -> &StorageObservation {
        &self.scratch
    }

    pub(crate) fn create_temporary(&self) -> Result<TempDir, SubjectError> {
        if StorageObservation::observe(self.scratch.path())? != self.scratch {
            return Err(SubjectError::Configuration(
                "scratch directory identity changed after admission".to_owned(),
            ));
        }
        let temporary = Builder::new().prefix("h3-").tempdir_in(self.scratch.path())?;
        if fs::metadata(temporary.path())?.dev() != self.scratch.device {
            return Err(SubjectError::Configuration(
                "disposable subject changed the selected scratch filesystem".to_owned(),
            ));
        }
        Ok(temporary)
    }
}

/// Observed canonical directory, filesystem device, and inode; not a storage-class assertion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StorageObservation {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl StorageObservation {
    pub(crate) fn observe(path: &Path) -> Result<Self, SubjectError> {
        let path = fs::canonicalize(path)?;
        if path.to_str().is_none() {
            return Err(SubjectError::Configuration(
                "scratch path must be representable exactly in UTF-8 configuration and evidence"
                    .to_owned(),
            ));
        }
        let metadata = fs::metadata(&path)?;
        if !metadata.is_dir() {
            return Err(SubjectError::Configuration(
                "scratch root must be an existing directory".to_owned(),
            ));
        }
        Ok(Self { path, device: metadata.dev(), inode: metadata.ino() })
    }

    /// Returns the observed canonical directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the operating system's filesystem device identity.
    #[must_use]
    pub const fn device(&self) -> u64 {
        self.device
    }
}

/// Actual workload placement retained only after process and directory cleanup succeed.
#[derive(Clone, Debug, Serialize)]
pub struct WorkloadStorage {
    workload_id: StableId,
    storage: StorageObservation,
    cleanup_completed: bool,
}

impl WorkloadStorage {
    pub(crate) const fn after_cleanup(workload_id: StableId, storage: StorageObservation) -> Self {
        Self { workload_id, storage, cleanup_completed: true }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt as _;

    use super::*;

    #[test]
    fn subjects_use_the_explicit_directory_and_release_their_owned_path() {
        let root = tempfile::tempdir().expect("scratch root");
        let config =
            SubjectConfiguration::new(&std::env::current_exe().expect("executable"), root.path())
                .expect("configuration");
        let first = config.create_temporary().expect("first subject");
        let second = config.create_temporary().expect("second subject");
        assert_ne!(first.path(), second.path());
        assert_eq!(first.path().parent(), Some(config.scratch().path()));
        assert_eq!(second.path().parent(), Some(config.scratch().path()));
        assert_eq!(fs::metadata(first.path()).expect("subject").dev(), config.scratch().device());
        first.close().expect("first cleanup");
        second.close().expect("second cleanup");
        assert_eq!(fs::read_dir(root.path()).expect("retained root").count(), 0);
    }

    #[test]
    fn substituted_or_non_directory_scratch_is_rejected() {
        let root = tempfile::tempdir().expect("root");
        let scratch = root.path().join("scratch");
        fs::create_dir(&scratch).expect("scratch");
        let executable = std::env::current_exe().expect("executable");
        let config = SubjectConfiguration::new(&executable, &scratch).expect("configuration");
        fs::rename(&scratch, root.path().join("original")).expect("retain original inode");
        fs::create_dir(&scratch).expect("replacement directory");
        assert!(matches!(config.create_temporary(), Err(SubjectError::Configuration(_))));
        let file = root.path().join("not-a-directory");
        fs::write(&file, b"file").expect("file");
        assert!(matches!(
            SubjectConfiguration::new(&executable, &file),
            Err(SubjectError::Configuration(_))
        ));
        assert!(SubjectConfiguration::new(&executable, &root.path().join("missing")).is_err());
    }

    #[test]
    fn scratch_path_cannot_be_lossily_reencoded_into_daemon_configuration() {
        let root = tempfile::tempdir().expect("root");
        let scratch = root.path().join(std::ffi::OsString::from_vec(vec![b's', 0xff]));
        fs::create_dir(&scratch).expect("non-UTF-8 directory");
        assert!(matches!(
            StorageObservation::observe(&scratch),
            Err(SubjectError::Configuration(_))
        ));
    }
}
