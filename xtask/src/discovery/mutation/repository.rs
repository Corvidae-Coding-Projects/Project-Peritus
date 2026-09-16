//! Short, private, committed-source copies for in-place mutation campaigns.

use super::runner;
use crate::error::XtaskError;
use std::fs;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_REPOSITORY: AtomicU64 = AtomicU64::new(0);

pub(super) struct MutationRepository {
    parent: Option<PathBuf>,
    repository: PathBuf,
}

impl MutationRepository {
    pub(super) fn clone(root: &Path, evidence: &Path) -> Result<Self, XtaskError> {
        let temporary_root = std::env::temp_dir();
        let temporary = temporary_root.canonicalize().map_err(|error| {
            XtaskError::io("canonicalize mutation temporary root", &temporary_root, error)
        })?;
        let canonical_root = root
            .canonicalize()
            .map_err(|error| XtaskError::io("canonicalize mutation source", root, error))?;
        if temporary.starts_with(canonical_root) {
            return Err(XtaskError::metadata(
                "mutation temporary root must be outside the Cargo workspace",
            ));
        }
        let mut identity = DefaultHasher::new();
        evidence.hash(&mut identity);
        root.hash(&mut identity);
        std::process::id().hash(&mut identity);
        NEXT_REPOSITORY.fetch_add(1, Ordering::Relaxed).hash(&mut identity);
        let parent = temporary.join(format!("pm-{:016x}", identity.finish()));
        fs::create_dir(&parent)
            .map_err(|error| XtaskError::io("create mutation temporary root", &parent, error))?;
        #[cfg(unix)]
        protect(&parent)?;
        let repository = parent.join("r");
        let mut clone = Command::new("git");
        clone.args(["clone", "--quiet", "--no-local", "--no-hardlinks"]).arg(root).arg(&repository);
        let owned = Self { parent: Some(parent), repository };
        runner::checked(root, evidence, "mutation-clone", clone, 60)?;
        if !owned.repository.join("Cargo.toml").is_file() {
            return Err(XtaskError::metadata("mutation clone has no workspace manifest"));
        }
        Ok(owned)
    }

    pub(super) fn path(&self) -> Result<&Path, XtaskError> {
        if self.parent.is_none() {
            return Err(XtaskError::metadata("mutation repository ownership lost"));
        }
        Ok(&self.repository)
    }

    pub(super) fn remove(&mut self) -> Result<(), XtaskError> {
        let parent = self
            .parent
            .as_ref()
            .ok_or_else(|| XtaskError::metadata("mutation repository ownership lost"))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match fs::remove_dir_all(parent) {
                Ok(()) => {
                    self.parent = None;
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.parent = None;
                    return Ok(());
                }
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    return Err(XtaskError::io(
                        "remove quiescent mutation repository",
                        parent,
                        error,
                    ));
                }
            }
        }
    }
}

impl Drop for MutationRepository {
    fn drop(&mut self) {
        if let Some(parent) = self.parent.take() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

#[cfg(unix)]
fn protect(path: &Path) -> Result<(), XtaskError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| XtaskError::io("protect mutation temporary root", path, error))
}
