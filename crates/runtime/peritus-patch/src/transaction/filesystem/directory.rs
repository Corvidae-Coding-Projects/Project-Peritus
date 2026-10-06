//! Directory-specific no-follow observations and permission staging.

use super::{
    Metadata, Observation, PatchError, PatchOperationContext, Path, RollbackStatus, TargetIdentity,
    fs, same_version, unsafe_target,
};
use crate::DirectoryMode;
#[cfg(not(windows))]
use std::fs::File;
use std::io;

pub(in crate::transaction) fn remove_observed(
    path: &Path,
    observed: Observation,
) -> io::Result<()> {
    match observed {
        Observation::Present(TargetIdentity::EmptyDirectory { .. }) => fs::remove_dir(path),
        Observation::Present(TargetIdentity::File { .. }) => fs::remove_file(path),
        Observation::Absent => Ok(()),
    }
}

pub(super) fn observe_empty_directory(
    path: &Path,
    before: &Metadata,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
) -> Result<Observation, PatchError> {
    #[cfg(not(windows))]
    let handle = File::open(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt as _;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0200_0000)
            .open(path)
            .map_err(|error| PatchError::io(operation, rollback, error))?
    };
    let opened = handle.metadata().map_err(|error| PatchError::io(operation, rollback, error))?;
    if !opened.is_dir() || !same_version(before, &opened) {
        return Err(unsafe_target(operation, rollback));
    }
    let mut entries =
        fs::read_dir(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    if entries
        .next()
        .transpose()
        .map_err(|error| PatchError::io(operation, rollback, error))?
        .is_some()
    {
        return Err(unsafe_target(operation, rollback));
    }
    let after =
        fs::symlink_metadata(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    let handle_after =
        handle.metadata().map_err(|error| PatchError::io(operation, rollback, error))?;
    if after.file_type().is_symlink()
        || !after.is_dir()
        || !same_version(before, &after)
        || !same_version(before, &handle_after)
    {
        return Err(unsafe_target(operation, rollback));
    }
    Ok(Observation::Present(TargetIdentity::EmptyDirectory { mode: directory_mode(&after) }))
}

#[cfg(unix)]
fn directory_mode(metadata: &Metadata) -> DirectoryMode {
    use std::os::unix::fs::PermissionsExt as _;
    DirectoryMode::new((metadata.permissions().mode() & 0o7777) as u16)
        .expect("masked permission bits")
}

#[cfg(not(unix))]
fn directory_mode(metadata: &Metadata) -> DirectoryMode {
    DirectoryMode::new(if metadata.permissions().readonly() { 0o555 } else { 0o777 })
        .expect("portable permission bits")
}

pub(in crate::transaction) fn set_directory_mode(
    path: &Path,
    mode: DirectoryMode,
) -> Result<(), PatchError> {
    let set = || {
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt as _;
            fs::Permissions::from_mode(u32::from(mode.bits()))
        };
        #[cfg(not(unix))]
        let permissions = {
            let mut permissions = fs::metadata(path)?.permissions();
            permissions.set_readonly(mode.bits() == 0o555);
            permissions
        };
        fs::set_permissions(path, permissions)
    };
    set().map_err(|error| {
        PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
    })
}
