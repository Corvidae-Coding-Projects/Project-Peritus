//! Exclusive daemon lock and exact record publication.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use super::record::InstanceRecord;
use crate::{DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery};

pub struct InstanceGuard {
    lock: File,
    record_path: PathBuf,
    record: InstanceRecord,
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
        Ok(Self { lock, record_path, record })
    }

    pub(crate) fn protocol_identity(
        &self,
        store_id: peritus_journal::StoreId,
        configuration_digest: peritus_types::Sha256Digest,
        executable_digest: peritus_types::Sha256Digest,
    ) -> Result<peritus_app_protocol::DaemonInstance, DaemonError> {
        peritus_app_protocol::DaemonInstance::new(
            *store_id.as_bytes(),
            configuration_digest,
            executable_digest,
            self.record.pid(),
            self.record.start_token(),
        )
        .map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "bind daemon protocol identity",
                "live instance record cannot be represented in the application protocol",
                error,
            )
        })
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if fs::read(&self.record_path).ok().as_deref() == Some(self.record.bytes()) {
            let _ = fs::remove_file(&self.record_path);
            if let Some(parent) = self.record_path.parent() {
                let _ = File::open(parent).and_then(|directory| directory.sync_all());
            }
        }
        // Closing only this handle can leave the lock held by a duplicate temporarily inherited
        // by a concurrently spawning child. Ownership ends here, before any successor starts.
        if let Err(error) = self.lock.unlock() {
            eprintln!("peritus daemon: could not release its instance lock: {error}");
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
        .and_then(|()| replace_record(&temporary, path))
        .and_then(|()| sync_directory(root));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(storage("publish daemon instance record", error));
    }
    Ok(())
}

#[cfg(unix)]
fn replace_record(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_record(temporary: &Path, path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(temporary, path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
const fn sync_directory(_path: &Path) -> std::io::Result<()> {
    // Windows does not support opening a directory through `File::open`; the record itself was
    // flushed before publication above.
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn released_owner_does_not_leave_its_lock_in_a_duplicated_handle() {
        let root = tempfile::tempdir().expect("state");
        let identity = DaemonIdentity::new(peritus_journal::StoreId::new([31; 16]).expect("store"));
        let owner = InstanceGuard::acquire(root.path(), &identity).expect("first owner");
        // Like a handle temporarily inherited between fork and exec, this duplicate shares the
        // open file description but has no daemon ownership authority of its own.
        let inherited = owner.lock.try_clone().expect("duplicate lock handle");
        assert!(InstanceGuard::acquire(root.path(), &identity).is_err());
        drop(owner);
        let successor = InstanceGuard::acquire(root.path(), &identity)
            .expect("completed owner explicitly releases the native lock");
        assert!(root.path().join("daemon.instance").is_file());
        drop(inherited);
        assert!(InstanceGuard::acquire(root.path(), &identity).is_err());
        drop(successor);
        assert!(!root.path().join("daemon.instance").exists());
    }
}
