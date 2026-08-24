//! Checked filesystem observations shared by application and recovery.

use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata},
    io::{self, Read},
    path::{Path, PathBuf},
};

use crate::{
    ErrorCode, FileMode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus,
    WorkspacePath,
};

use super::manifest::FileIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Observation {
    Absent,
    Present(FileIdentity),
    Oversized,
}

pub(super) fn observe_target(
    workspace: &Path,
    path: &WorkspacePath,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
) -> Result<Observation, PatchError> {
    let target = checked_target_path(workspace, path, operation, rollback)?;
    observe_absolute(&target, operation, rollback).map_err(|error| error.at(path.clone()))
}

pub(super) fn observe_absolute(
    path: &Path,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
) -> Result<Observation, PatchError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Observation::Absent),
        Err(error) => return Err(PatchError::io(operation, rollback, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(unsafe_target(operation, rollback));
    }
    if metadata.len() > crate::set::MAX_FILE_BYTES as u64 {
        return Ok(Observation::Oversized);
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| {
        PatchError::message(
            ErrorCode::ArithmeticOverflow,
            RecoveryClass::FenceWorkspace,
            operation,
            rollback,
            "observed file size cannot be represented",
        )
    })?;
    let file = File::open(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(crate::set::MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| PatchError::io(operation, rollback, error))?;
    let after =
        fs::symlink_metadata(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    if after.file_type().is_symlink() || !after.is_file() {
        return Err(unsafe_target(operation, rollback));
    }
    if bytes.len() > crate::set::MAX_FILE_BYTES || after.len() > crate::set::MAX_FILE_BYTES as u64 {
        return Ok(Observation::Oversized);
    }
    let size = u64::try_from(bytes.len()).map_err(|_| {
        PatchError::message(
            ErrorCode::ArithmeticOverflow,
            RecoveryClass::FenceWorkspace,
            operation,
            rollback,
            "observed file size cannot be represented",
        )
    })?;
    Ok(Observation::Present(FileIdentity {
        digest: peritus_codec::sha256(&bytes),
        size,
        mode: mode_from_metadata(&after),
    }))
}

pub(super) fn observation_matches(observed: Observation, expected: Option<FileIdentity>) -> bool {
    match (observed, expected) {
        (Observation::Absent, None) => true,
        (Observation::Present(observed), Some(expected)) => crate::verified::file_identity_matches(
            expected.size,
            observed.size,
            expected.digest == observed.digest,
            expected.mode.tag(),
            observed.mode.tag(),
        ),
        _ => false,
    }
}

pub(super) fn discover_missing_directories(
    workspace: &Path,
    paths: impl Iterator<Item = WorkspacePath>,
) -> Result<Vec<WorkspacePath>, PatchError> {
    let mut missing = BTreeSet::new();
    for path in paths {
        let components: Vec<_> = path.components().collect();
        let mut relative = String::new();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !relative.is_empty() {
                relative.push('/');
            }
            relative.push_str(component);
            let directory = WorkspacePath::new(relative.clone())?;
            let absolute = workspace.join(directory.as_path());
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    reject_nested_repository(&absolute)?;
                }
                Ok(_) => {
                    return Err(unsafe_target(
                        PatchOperationContext::Prepare,
                        RollbackStatus::NotRequired,
                    )
                    .at(directory));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    missing.insert(directory);
                }
                Err(error) => {
                    return Err(PatchError::io(
                        PatchOperationContext::Prepare,
                        RollbackStatus::NotRequired,
                        error,
                    )
                    .at(directory));
                }
            }
        }
    }
    let mut missing: Vec<_> = missing.into_iter().collect();
    missing.sort_by(|left, right| {
        left.components().count().cmp(&right.components().count()).then_with(|| left.cmp(right))
    });
    Ok(missing)
}

pub(super) fn create_directory(
    workspace: &Path,
    directory: &WorkspacePath,
    mutated: &mut bool,
) -> Result<(), PatchError> {
    let absolute = checked_target_path(
        workspace,
        directory,
        PatchOperationContext::InstallFinal,
        RollbackStatus::Indeterminate,
    )?;
    match fs::create_dir(&absolute) {
        Ok(()) => {
            *mutated = true;
            if let Some(parent) = absolute.parent() {
                sync_directory(parent, RollbackStatus::Indeterminate)?;
            }
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(PatchError::message(
                ErrorCode::PreimageUnexpected,
                RecoveryClass::FenceWorkspace,
                PatchOperationContext::InstallFinal,
                RollbackStatus::Indeterminate,
                "a directory declared absent appeared during installation",
            )
            .at(directory.clone()));
        }
        Err(error) => {
            return Err(PatchError::io(
                PatchOperationContext::InstallFinal,
                RollbackStatus::Indeterminate,
                error,
            )
            .at(directory.clone()));
        }
    }
    Ok(())
}

pub(super) fn checked_target_path(
    workspace: &Path,
    path: &WorkspacePath,
    operation: PatchOperationContext,
    rollback: RollbackStatus,
) -> Result<PathBuf, PatchError> {
    let mut cursor = workspace.to_path_buf();
    let component_count = path.components().count();
    for (index, component) in path.components().enumerate() {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || (index + 1 < component_count && !metadata.is_dir())
                {
                    return Err(unsafe_target(operation, rollback).at(path.clone()));
                }
                if index + 1 < component_count {
                    reject_nested_repository(&cursor)?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(PatchError::io(operation, rollback, error).at(path.clone())),
        }
    }
    Ok(cursor)
}

fn reject_nested_repository(directory: &Path) -> Result<(), PatchError> {
    match fs::symlink_metadata(directory.join(".git")) {
        Ok(_) => {
            Err(unsafe_target(PatchOperationContext::InspectPreimage, RollbackStatus::NotRequired))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PatchError::io(
            PatchOperationContext::InspectPreimage,
            RollbackStatus::NotRequired,
            error,
        )),
    }
}

pub(super) fn set_mode(path: &Path, mode: FileMode) -> Result<(), PatchError> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
        })?
        .permissions();
    set_portable_mode(&mut permissions, mode);
    fs::set_permissions(path, permissions).map_err(|error| {
        PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
    })
}

#[cfg(unix)]
fn set_portable_mode(permissions: &mut fs::Permissions, mode: FileMode) {
    use std::os::unix::fs::PermissionsExt as _;
    permissions.set_mode(match mode {
        FileMode::Regular => 0o644,
        FileMode::Executable => 0o755,
    });
}

#[cfg(not(unix))]
fn set_portable_mode(permissions: &mut fs::Permissions, _mode: FileMode) {
    permissions.set_readonly(false);
}

#[cfg(unix)]
fn mode_from_metadata(metadata: &Metadata) -> FileMode {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o111 == 0 {
        FileMode::Regular
    } else {
        FileMode::Executable
    }
}

#[cfg(not(unix))]
const fn mode_from_metadata(_metadata: &Metadata) -> FileMode {
    FileMode::Regular
}

pub(super) fn sync_directory(directory: &Path, rollback: RollbackStatus) -> Result<(), PatchError> {
    File::open(directory).and_then(|file| file.sync_all()).map_err(|error| {
        PatchError::io(PatchOperationContext::SynchronizeDirectory, rollback, error)
    })
}

pub(super) fn remove_created_directories(
    workspace: &Path,
    directories: &[WorkspacePath],
) -> Result<(), PatchError> {
    for directory in directories.iter().rev() {
        let absolute = checked_target_path(
            workspace,
            directory,
            PatchOperationContext::Rollback,
            RollbackStatus::Indeterminate,
        )?;
        match fs::remove_dir(&absolute) {
            Ok(()) => {
                if let Some(parent) = absolute.parent() {
                    sync_directory(parent, RollbackStatus::Indeterminate)?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(PatchError::io(
                    PatchOperationContext::Rollback,
                    RollbackStatus::Indeterminate,
                    error,
                )
                .at(directory.clone()));
            }
        }
    }
    Ok(())
}

const fn unsafe_target(operation: PatchOperationContext, rollback: RollbackStatus) -> PatchError {
    PatchError::message(
        ErrorCode::UnsafeFilesystemTarget,
        RecoveryClass::FenceWorkspace,
        operation,
        rollback,
        "target or ancestor is a symlink, special node, or nested repository",
    )
}
