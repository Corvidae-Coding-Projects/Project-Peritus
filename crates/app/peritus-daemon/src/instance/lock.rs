//! Exclusive daemon lock and exact record publication.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use super::record::InstanceRecord;
use crate::{DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery};

pub struct InstanceGuard {
    _lock: File,
    record_path: PathBuf,
    record_bytes: Vec<u8>,
}

impl InstanceGuard {
    pub(crate) fn acquire(
        state_root: &Path,
        identity: &DaemonIdentity,
    ) -> Result<Self, DaemonError> {
        prepare_state_root(state_root)?;
        let lock_path = state_root.join("daemon.lock");
        let lock = open_private(&lock_path)?;
        lock.try_lock().map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::AlreadyRunning,
                DaemonRecovery::Retry,
                "acquire daemon instance lock",
                "another live daemon owns this store identity",
                error,
            )
        })?;
        let record = InstanceRecord::current(identity)?;
        let record_path = state_root.join("daemon.instance");
        publish_record(state_root, &record_path, record.bytes())?;
        Ok(Self { _lock: lock, record_path, record_bytes: record.bytes().to_vec() })
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if fs::read(&self.record_path).ok().as_deref() == Some(self.record_bytes.as_slice()) {
            let _ = fs::remove_file(&self.record_path);
            if let Some(parent) = self.record_path.parent() {
                let _ = File::open(parent).and_then(|directory| directory.sync_all());
            }
        }
    }
}

fn prepare_state_root(path: &Path) -> Result<(), DaemonError> {
    fs::create_dir_all(path).map_err(|error| storage("create daemon state root", error))?;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| storage("inspect daemon state root", error))?;
    if !metadata.file_type().is_dir() {
        return Err(DaemonError::new(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            "validate daemon state root",
            "state root is not a directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.mode() & 0o777;
        if mode & 0o077 != 0 {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|error| storage("protect daemon state root", error))?;
        }
    }
    Ok(())
}

fn open_private(path: &Path) -> Result<File, DaemonError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|error| storage("open daemon instance lock", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| storage("protect daemon instance lock", error))?;
    }
    Ok(file)
}

fn publish_record(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), DaemonError> {
    let temporary = root.join(format!(".daemon.instance.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| storage("create daemon instance record", error))?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temporary, path))
        .and_then(|()| File::open(root).and_then(|directory| directory.sync_all()));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(storage("publish daemon instance record", error));
    }
    Ok(())
}

fn storage(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Retry,
        operation,
        "daemon instance filesystem operation failed",
        error,
    )
}
