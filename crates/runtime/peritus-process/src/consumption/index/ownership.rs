//! Kernel-owned scratch indexes can be reclaimed after owner loss without touching live indexes.

use std::{fs::{self, File, OpenOptions, TryLockError}, path::{Path, PathBuf}};

use crate::ProcessError;
use super::super::{store_cause, store_error};

pub(super) struct IndexOwner {
    _file: File,
    path: PathBuf,
}

impl IndexOwner {
    pub(super) fn acquire(root: &Path, index: &Path) -> Result<Self, ProcessError> {
        let name = index.file_name().and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| store_error("process index has no canonical filename"))?;
        let path = root.join(format!("{name}.lease"));
        // Publish the owner inode only after it is locked. Another opener can never mistake the
        // create-to-lock interval for a dead owner and delete a newly created live index.
        let temporary = tempfile::Builder::new().prefix(".process-index-owner-staging-")
            .tempfile_in(root).map_err(|error| store_cause("process index owner cannot be created", error))?;
        temporary.as_file().try_lock()
            .map_err(|error| store_cause("process index owner cannot be locked", error))?;
        let file = temporary.persist_noclobber(&path)
            .map_err(|error| store_cause("process index owner cannot be published", error))?;
        Ok(Self { _file: file, path })
    }
}

impl Drop for IndexOwner {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn retire_abandoned_indexes(root: &Path) -> Result<(), ProcessError> {
    for entry in fs::read_dir(root).map_err(|error| store_cause("process indexes cannot be inspected", error))? {
        let entry = entry.map_err(|error| store_cause("process index entry cannot be inspected", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue; };
        let Some(index_name) = name.strip_suffix(".lease") else { continue; };
        if !index_name.starts_with(".process-index-data-") { continue; }
        if !entry.file_type().map_err(|error| store_cause("process index owner type is unavailable", error))?.is_file() {
            continue;
        }
        let path = entry.path();
        let file = match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(store_error("process index owner cannot be inspected")),
        };
        match file.try_lock() {
            Err(TryLockError::WouldBlock) => continue,
            Err(_) => return Err(store_error("process index owner cannot be observed")),
            Ok(()) => {}
        }
        // This exact kernel-owned lease has no live owner. SQLite has no shared connection to its
        // private scratch file; neither history receipts nor process spools are cleanup candidates.
        for target in [root.join(index_name), root.join(format!("{index_name}-journal"))] {
            match fs::symlink_metadata(&target) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    fs::remove_file(target).map_err(|error| store_cause("abandoned process index cannot be removed", error))?;
                }
                Ok(_) => return Err(store_error("abandoned process index is not a regular file")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(store_error("abandoned process index cannot be inspected")),
            }
        }
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(store_error("abandoned process index owner cannot be removed")),
        }
    }
    Ok(())
}
