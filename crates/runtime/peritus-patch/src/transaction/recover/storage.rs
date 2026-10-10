use std::{
    fs, io,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{PatchError, PatchOperationContext, RollbackStatus};

use super::super::{filesystem::sync_directory, storage::MANIFEST_FILE};

static NEXT_QUARANTINE: AtomicU64 = AtomicU64::new(0);

pub(super) fn read_manifest(transaction_directory: &Path) -> io::Result<Vec<u8>> {
    let path = transaction_directory.join(MANIFEST_FILE);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unsafe manifest file"));
    }
    fs::read(path)
}

pub(super) fn quarantine(transaction_directory: &Path) -> Result<bool, PatchError> {
    let parent = transaction_directory
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Recover))?;
    let name = transaction_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Recover))?;
    loop {
        let sequence = NEXT_QUARANTINE.fetch_add(1, Ordering::Relaxed);
        let nanos =
            SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_nanos());
        let quarantine_name =
            format!("{name}.quarantine-{}-{nanos:x}-{sequence:x}", std::process::id());
        let destination = parent.join(quarantine_name);
        match fs::rename(transaction_directory, &destination) {
            Ok(()) => {
                sync_directory(parent, RollbackStatus::Indeterminate)?;
                return Ok(true);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(PatchError::io(
                    PatchOperationContext::Recover,
                    RollbackStatus::Indeterminate,
                    error,
                ));
            }
        }
    }
}
