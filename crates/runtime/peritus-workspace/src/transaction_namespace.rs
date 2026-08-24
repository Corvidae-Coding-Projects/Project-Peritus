//! Canonical target-owned transaction namespaces and durable identity binding.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use crate::{ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation, WorkspaceState};

const MAGIC: &[u8] = b"PERITUS-WORKSPACE-TRANSACTION-NAMESPACE-V1\0";
const BINDING_FILE: &str = "binding.bin";

pub fn open(
    requested_root: PathBuf,
    state: &WorkspaceState,
    worktree_root: &Path,
    common_dir: &Path,
) -> Result<PathBuf, WorkspaceError> {
    if !requested_root.is_absolute()
        || overlaps(&requested_root, worktree_root)
        || overlaps(&requested_root, common_dir)
    {
        return Err(namespace_error(
            "transaction root overlaps the worktree or Git common directory",
        ));
    }
    fs::create_dir_all(&requested_root)
        .map_err(|_| namespace_error("transaction root cannot be created"))?;
    let root = fs::canonicalize(requested_root)
        .map_err(|_| namespace_error("transaction root cannot be canonicalized"))?;
    if overlaps(&root, worktree_root) || overlaps(&root, common_dir) {
        return Err(namespace_error(
            "canonical transaction root overlaps the worktree or Git common directory",
        ));
    }
    let namespace = root.join(namespace_name(state));
    match fs::create_dir(&namespace) {
        Ok(()) => crate::filesystem::sync_directory(&root)
            .map_err(|_| namespace_error("transaction namespace parent cannot be synchronized"))?,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(_) => return Err(namespace_error("transaction namespace cannot be created")),
    }
    let metadata = fs::symlink_metadata(&namespace)
        .map_err(|_| namespace_error("transaction namespace cannot be inspected"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(namespace_error("transaction namespace is not a real directory"));
    }
    let namespace = fs::canonicalize(namespace)
        .map_err(|_| namespace_error("transaction namespace cannot be canonicalized"))?;
    if namespace.parent() != Some(root.as_path())
        || overlaps(&namespace, worktree_root)
        || overlaps(&namespace, common_dir)
    {
        return Err(namespace_error("transaction namespace escaped its isolated root"));
    }
    establish_binding(&namespace, state)?;
    Ok(namespace)
}

pub fn binding_manifest_path(namespace: &Path) -> PathBuf {
    namespace.join(BINDING_FILE)
}

pub fn binding_is_exact(namespace: &Path, state: &WorkspaceState) -> bool {
    let path = binding_manifest_path(namespace);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    fs::read(path).is_ok_and(|bytes| bytes == binding_bytes(state))
}

pub fn is_canonical_transaction_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
        return false;
    };
    let Some(identity) = name.strip_prefix("txn-") else {
        return false;
    };
    identity.len() == 64
        && identity.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn establish_binding(namespace: &Path, state: &WorkspaceState) -> Result<(), WorkspaceError> {
    let path = binding_manifest_path(namespace);
    let expected = binding_bytes(state);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut marker) => {
            marker
                .write_all(&expected)
                .and_then(|()| marker.sync_all())
                .map_err(|_| namespace_error("transaction namespace binding cannot be written"))?;
            crate::filesystem::sync_directory(namespace).map_err(|_| {
                namespace_error("transaction namespace binding cannot be synchronized")
            })?;
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if binding_is_exact(namespace, state) {
                Ok(())
            } else {
                Err(namespace_error(
                    "transaction namespace binding differs from the exact workspace target",
                ))
            }
        }
        Err(_) => Err(namespace_error("transaction namespace binding cannot be created")),
    }
}

fn namespace_name(state: &WorkspaceState) -> String {
    let digest = peritus_codec::sha256(&binding_bytes(state));
    let mut name = String::from("workspace-");
    for byte in digest.as_bytes() {
        use core::fmt::Write as _;
        write!(&mut name, "{byte:02x}").expect("writing to String is infallible");
    }
    name
}

fn binding_bytes(state: &WorkspaceState) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(MAGIC.len() + 48);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(state.binding().workspace_id().as_bytes());
    bytes.extend_from_slice(state.binding().resource_id().as_bytes());
    bytes.extend_from_slice(state.binding().environment_id().as_bytes());
    bytes
}

fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

const fn namespace_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::InvalidInput,
        WorkspaceOperation::Open,
        RecoveryClass::CorrectRequest,
        detail,
    )
}
