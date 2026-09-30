//! Recoverable removal of repositories that did not exist at task admission.

use super::transaction::{Journal, validation_directory_digest};
use super::{ManagedBaseline, failure, git, text};
use crate::ProductRunnerError;
use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

impl ManagedBaseline {
    pub(super) fn new_repositories(
        &self,
        root: &Path,
        paths: &[PathBuf],
    ) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let mut ordered = paths.to_vec();
        ordered.sort_by_key(|path| path.components().count());
        let mut repositories = Vec::new();
        for path in ordered {
            super::validate_path(&path)?;
            if self.nested.keys().any(|prefix| path.starts_with(prefix))
                || repositories.iter().any(|prefix| path.starts_with(prefix))
                || !super::paths::parent_is_directory(root, &path)?
            {
                continue;
            }
            let absolute = root.join(&path);
            if fs::symlink_metadata(&absolute).is_ok_and(|metadata| metadata.is_dir())
                && fs::symlink_metadata(absolute.join(".git")).is_ok()
            {
                repositories.push(path);
            }
        }
        Ok(repositories)
    }

    pub(super) fn archive_new_repositories(
        &self,
        root: &Path,
        paths: &[PathBuf],
        mut journal: Option<&mut Journal>,
    ) -> Result<Vec<PathBuf>, ProductRunnerError> {
        self.new_repositories(root, paths)?
            .into_iter()
            .map(|path| archive_observed(root, &path, journal.as_deref_mut()))
            .collect()
    }
}

pub(super) fn archive_observed(
    root: &Path,
    relative: &Path,
    journal: Option<&mut Journal>,
) -> Result<PathBuf, ProductRunnerError> {
    let source = root.join(relative).canonicalize().map_err(failure)?;
    let git_directory =
        PathBuf::from(text(git(root, &["rev-parse", "--absolute-git-dir"], None)?)?)
            .canonicalize()
            .map_err(failure)?;
    if git_directory.starts_with(&source) {
        return Err(failure("recovery directory cannot be inside the repository being discarded"));
    }
    let owner = git_directory.join("peritus");
    create_directory(&owner)?;
    let directory = owner.join("discarded");
    create_directory(&directory)?;
    let temporary =
        tempfile::Builder::new().prefix("repository-").tempdir_in(&directory).map_err(failure)?;
    let mut origin = fs::File::create(temporary.path().join("origin.json")).map_err(failure)?;
    let metadata = serde_json::json!({"original_path": source, "saved_directory": "repository"});
    origin.write_all(&serde_json::to_vec_pretty(&metadata).map_err(failure)?).map_err(failure)?;
    origin.sync_all().map_err(failure)?;
    sync_directory(temporary.path())?;
    // Once a source directory can move here, no automatic cleanup may remove it.
    let recovery = temporary.keep();
    sync_directory(&directory)?;
    if let Some(journal) = journal {
        journal.recovery(
            recovery.clone(),
            Some(source.clone()),
            Some(validation_directory_digest(&source)?),
        )?;
    }
    fs::rename(&source, recovery.join("repository")).map_err(|error| {
        failure(format!(
            "could not preserve {} in {}: {error}",
            source.display(),
            recovery.display()
        ))
    })?;
    let durable = || {
        sync_directory(&recovery)?;
        sync_directory(source.parent().ok_or_else(|| failure("repository has no parent"))?)
    };
    durable().map_err(|error| {
        failure(format!(
            "repository preserved at {}, but recovery synchronization failed: {error}",
            recovery.display()
        ))
    })?;
    #[cfg(test)]
    super::transaction::fault::pause(super::transaction::fault::Stage::Archive);
    Ok(recovery)
}

pub(super) fn create_directory(path: &Path) -> Result<(), ProductRunnerError> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(failure(error)),
    }
    if !fs::symlink_metadata(path).map_err(failure)?.is_dir() {
        return Err(failure("recovery directory is not an ordinary directory"));
    }
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

#[allow(
    clippy::missing_const_for_fn,
    clippy::unnecessary_wraps,
    reason = "shared fallible filesystem interface; directory fsync is available only on Unix"
)]
pub(super) fn sync_directory(path: &Path) -> Result<(), ProductRunnerError> {
    #[cfg(unix)]
    fs::File::open(path).and_then(|directory| directory.sync_all()).map_err(failure)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
