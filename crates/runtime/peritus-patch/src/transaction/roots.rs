//! Workspace and protected transaction root validation.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{ErrorCode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus};

#[derive(Debug)]
pub(super) struct Roots {
    pub(super) workspace: PathBuf,
    pub(super) transaction_root: PathBuf,
}

pub(super) fn prepare_roots(
    workspace: &Path,
    transaction_root: &Path,
) -> Result<Roots, PatchError> {
    check_existing_directory(workspace)?;
    fs::create_dir_all(transaction_root).map_err(prepare_io)?;
    check_existing_directory(transaction_root)?;
    let workspace = fs::canonicalize(workspace).map_err(prepare_io)?;
    let transaction_root = fs::canonicalize(transaction_root).map_err(prepare_io)?;
    validate_disjoint_same_device(&workspace, &transaction_root)?;
    Ok(Roots { workspace, transaction_root })
}

pub(super) fn recovery_roots(
    workspace: &Path,
    transaction_directory: &Path,
) -> Result<(PathBuf, PathBuf), PatchError> {
    check_existing_directory(workspace)?;
    check_existing_directory(transaction_directory)?;
    let workspace = fs::canonicalize(workspace).map_err(prepare_io)?;
    let transaction_directory = fs::canonicalize(transaction_directory).map_err(prepare_io)?;
    validate_disjoint_same_device(&workspace, &transaction_directory)?;
    Ok((workspace, transaction_directory))
}

fn validate_disjoint_same_device(left: &Path, right: &Path) -> Result<(), PatchError> {
    if left.starts_with(right) || right.starts_with(left) || !same_device(left, right)? {
        Err(invalid_roots())
    } else {
        Ok(())
    }
}

fn check_existing_directory(path: &Path) -> Result<(), PatchError> {
    let metadata = fs::symlink_metadata(path).map_err(prepare_io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_roots());
    }
    Ok(())
}

#[cfg(unix)]
fn same_device(left: &Path, right: &Path) -> Result<bool, PatchError> {
    use std::os::unix::fs::MetadataExt as _;
    let left = fs::metadata(left).map_err(prepare_io)?;
    let right = fs::metadata(right).map_err(prepare_io)?;
    Ok(left.dev() == right.dev())
}

#[cfg(not(unix))]
fn same_device(_left: &Path, _right: &Path) -> Result<bool, PatchError> {
    Ok(true)
}

fn prepare_io(error: io::Error) -> PatchError {
    PatchError::io(PatchOperationContext::Prepare, RollbackStatus::NotRequired, error)
}

const fn invalid_roots() -> PatchError {
    PatchError::message(
        ErrorCode::InvalidTransactionRoot,
        RecoveryClass::CorrectPatch,
        PatchOperationContext::Prepare,
        RollbackStatus::NotRequired,
        "workspace and transaction roots must be separate safe directories on one device",
    )
}
