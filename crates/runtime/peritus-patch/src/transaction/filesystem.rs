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

use super::manifest::TargetIdentity;

mod directory;
use directory::observe_empty_directory;
pub(super) use directory::{remove_observed, set_directory_mode};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Observation {
    Absent,
    Present(TargetIdentity),
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
    if metadata.file_type().is_symlink() {
        return Err(unsafe_target(operation, rollback));
    }
    if metadata.is_dir() {
        return observe_empty_directory(path, &metadata, operation, rollback);
    }
    if !metadata.is_file() {
        return Err(unsafe_target(operation, rollback));
    }
    let mut file = File::open(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    let opened = file.metadata().map_err(|error| PatchError::io(operation, rollback, error))?;
    if !same_version(&metadata, &opened) {
        return Err(unsafe_target(operation, rollback));
    }
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let count =
            file.read(&mut chunk).map_err(|error| PatchError::io(operation, rollback, error))?;
        if count == 0 {
            break;
        }
        size = size.checked_add(count as u64).ok_or_else(|| unsafe_target(operation, rollback))?;
        if size > metadata.len() {
            return Err(unsafe_target(operation, rollback));
        }
        hasher.update(&chunk[..count]);
    }
    let after =
        fs::symlink_metadata(path).map_err(|error| PatchError::io(operation, rollback, error))?;
    let handle_after =
        file.metadata().map_err(|error| PatchError::io(operation, rollback, error))?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || size != metadata.len()
        || !same_version(&metadata, &after)
        || !same_version(&metadata, &handle_after)
    {
        return Err(unsafe_target(operation, rollback));
    }
    Ok(Observation::Present(TargetIdentity::File {
        digest: Sha256Digest::new(hasher.finalize().into()),
        size,
        mode: mode_from_metadata(&after),
    }))
}

fn same_version(left: &Metadata, right: &Metadata) -> bool {
    let same = left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && left.permissions() == right.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        same && left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        same
    }
}

pub(super) fn observation_matches(observed: Observation, expected: Option<TargetIdentity>) -> bool {
    match (observed, expected) {
        (Observation::Absent, None) => true,
        (
            Observation::Present(TargetIdentity::File { digest, size, mode }),
            Some(TargetIdentity::File {
                digest: expected_digest,
                size: expected_size,
                mode: expected_mode,
            }),
        ) => crate::verified::file_identity_matches(
            expected_size,
            size,
            expected_digest == digest,
            expected_mode.tag(),
            mode.tag(),
        ),
        (
            Observation::Present(TargetIdentity::EmptyDirectory { mode }),
            Some(TargetIdentity::EmptyDirectory { mode: expected_mode }),
        ) => mode == expected_mode,
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
    let permissions = fs::metadata(path)
        .map_err(|error| {
            PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
        })?
        .permissions();
    fs::set_permissions(path, portable_permissions(permissions, mode)).map_err(|error| {
        PatchError::io(PatchOperationContext::StageFinal, RollbackStatus::NotRequired, error)
    })
}

#[cfg(unix)]
fn portable_permissions(mut permissions: fs::Permissions, mode: FileMode) -> fs::Permissions {
    use std::os::unix::fs::PermissionsExt as _;
    let bits = permissions.mode();
    let execute = match mode {
        FileMode::Regular => 0,
        FileMode::Executable if bits & 0o111 != 0 => bits & 0o111,
        FileMode::Executable => (bits & 0o444) >> 2 | 0o100,
    };
    permissions.set_mode((bits & !0o111) | execute);
    permissions
}

pub(super) fn preserve_replacement_permissions(
    staged: &Path,
    backup: &Path,
    mode: FileMode,
) -> Result<(), PatchError> {
    let copy = || {
        let permissions = portable_permissions(fs::metadata(backup)?.permissions(), mode);
        #[cfg(windows)]
        let file = fs::OpenOptions::new().write(true).open(staged)?;
        #[cfg(not(windows))]
        let file = File::open(staged)?;
        fs::set_permissions(staged, permissions)?;
        file.sync_all()
    };
    copy().map_err(|error| {
        PatchError::io(PatchOperationContext::InstallFinal, RollbackStatus::Indeterminate, error)
    })
}

#[cfg(not(unix))]
const fn portable_permissions(permissions: fs::Permissions, _mode: FileMode) -> fs::Permissions {
    // Executable mode is rejected before staging on these platforms; preserve readonly.
    permissions
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
    sync_directory_os(directory).map_err(|error| {
        PatchError::io(PatchOperationContext::SynchronizeDirectory, rollback, error)
    })
}

#[cfg(not(windows))]
fn sync_directory_os(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(windows)]
fn sync_directory_os(directory: &Path) -> io::Result<()> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt as _};

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(directory)?
        .sync_all()
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
