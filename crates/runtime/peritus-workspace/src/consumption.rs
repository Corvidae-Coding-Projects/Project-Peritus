//! Durable per-revision action-consumption markers owned by the writable target.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use peritus_types::{ActionId, Sha256Digest};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation, WorkspaceState, WritableWorkspace,
};

const MAGIC: &[u8] = b"PERITUS-WORKSPACE-ACTION-V1\0";
const MARKER_BYTES: usize = MAGIC.len() + 16 + 16 + 16 + 8 + 8 + 16 + 32;
const MAX_ACTIONS_PER_REVISION: usize = 1_024;

impl WritableWorkspace {
    pub(crate) fn restore_action_consumption(&mut self) -> Result<(), WorkspaceError> {
        let directory = revision_directory(self.transaction_root(), self.state());
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(consumption_error("action ledger cannot be inspected")),
        };
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(consumption_error("action ledger is not a real directory"));
        }
        let canonical = fs::canonicalize(&directory)
            .map_err(|_| consumption_error("action ledger cannot be canonicalized"))?;
        if !canonical.starts_with(self.transaction_root()) {
            return Err(consumption_error("action ledger escaped its transaction root"));
        }
        let entries = fs::read_dir(&canonical)
            .map_err(|_| consumption_error("action ledger cannot be read"))?;
        let mut count = 0_usize;
        for entry in entries {
            count = count
                .checked_add(1)
                .ok_or_else(|| consumption_error("action ledger entry count overflowed"))?;
            if count > MAX_ACTIONS_PER_REVISION {
                return Err(consumption_error("action ledger exceeds its per-revision bound"));
            }
            let entry = entry.map_err(|_| consumption_error("action marker cannot be listed"))?;
            let file_type = entry
                .file_type()
                .map_err(|_| consumption_error("action marker type cannot be inspected"))?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(consumption_error("action marker is not a regular file"));
            }
            let bytes = fs::read(entry.path())
                .map_err(|_| consumption_error("action marker cannot be read"))?;
            let (action_id, action_digest) = decode_marker(self.state(), &bytes)?;
            let expected_name = marker_name(action_id);
            if entry.file_name() != std::ffi::OsStr::new(&expected_name) {
                return Err(consumption_error("action marker name differs from its identity"));
            }
            self.state_mut().record_consumed_action(action_id, action_digest);
        }
        Ok(())
    }

    pub(crate) fn commit_action_consumption(
        &mut self,
        action_id: ActionId,
        action_digest: Sha256Digest,
    ) -> Result<(), WorkspaceError> {
        if self.state().action_consumed(action_id) {
            return Err(reused_error());
        }
        if self.state().consumed_action_count() >= MAX_ACTIONS_PER_REVISION {
            return Err(consumption_error("action ledger exceeds its per-revision bound"));
        }
        let directory = revision_directory(self.transaction_root(), self.state());
        create_checked_directory(self.transaction_root(), &directory)?;
        let path = directory.join(marker_name(action_id));
        let mut marker = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(marker) => marker,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(reused_error()),
            Err(_) => return Err(consumption_error("action marker cannot be created exclusively")),
        };
        let bytes = encode_marker(self.state(), action_id, action_digest);
        marker
            .write_all(&bytes)
            .and_then(|()| marker.sync_all())
            .map_err(|_| consumption_error("action marker cannot be synchronized"))?;
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| consumption_error("action ledger directory cannot be synchronized"))?;
        self.state_mut().record_consumed_action(action_id, action_digest);
        Ok(())
    }
}

fn create_checked_directory(root: &Path, directory: &Path) -> Result<(), WorkspaceError> {
    fs::create_dir_all(directory)
        .map_err(|_| consumption_error("action ledger directory cannot be created"))?;
    let metadata = fs::symlink_metadata(directory)
        .map_err(|_| consumption_error("action ledger directory cannot be inspected"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(consumption_error("action ledger path is not a real directory"));
    }
    let canonical = fs::canonicalize(directory)
        .map_err(|_| consumption_error("action ledger directory cannot be canonicalized"))?;
    if !canonical.starts_with(root) {
        return Err(consumption_error("action ledger directory escaped its transaction root"));
    }
    Ok(())
}

fn revision_directory(root: &Path, state: &WorkspaceState) -> PathBuf {
    action_ledger_root(root).join(format!(
        "generation-{}-revision-{}",
        state.generation().get(),
        state.revision().get()
    ))
}

pub fn action_ledger_root(root: &Path) -> PathBuf {
    root.join("workspace-actions-v1")
}

fn marker_name(action_id: ActionId) -> String {
    let mut result = String::with_capacity(ActionId::LENGTH * 2);
    for byte in action_id.as_bytes() {
        use core::fmt::Write as _;
        write!(&mut result, "{byte:02x}").expect("writing to String is infallible");
    }
    result
}

fn encode_marker(
    state: &WorkspaceState,
    action_id: ActionId,
    action_digest: Sha256Digest,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(MARKER_BYTES);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(state.binding().workspace_id().as_bytes());
    bytes.extend_from_slice(state.binding().resource_id().as_bytes());
    bytes.extend_from_slice(state.binding().environment_id().as_bytes());
    bytes.extend_from_slice(&state.generation().get().to_be_bytes());
    bytes.extend_from_slice(&state.revision().get().to_be_bytes());
    bytes.extend_from_slice(action_id.as_bytes());
    bytes.extend_from_slice(action_digest.as_bytes());
    bytes
}

fn decode_marker(
    state: &WorkspaceState,
    bytes: &[u8],
) -> Result<(ActionId, Sha256Digest), WorkspaceError> {
    if bytes.len() != MARKER_BYTES || !bytes.starts_with(MAGIC) {
        return Err(consumption_error("action marker has invalid canonical bytes"));
    }
    let mut offset = MAGIC.len();
    let workspace = take_array::<16>(bytes, &mut offset);
    let resource = take_array::<16>(bytes, &mut offset);
    let environment = take_array::<16>(bytes, &mut offset);
    let generation = u64::from_be_bytes(take_array::<8>(bytes, &mut offset));
    let revision = u64::from_be_bytes(take_array::<8>(bytes, &mut offset));
    let action = take_array::<16>(bytes, &mut offset);
    let digest = take_array::<32>(bytes, &mut offset);
    if workspace != state.binding().workspace_id().into_bytes()
        || resource != state.binding().resource_id().into_bytes()
        || environment != state.binding().environment_id().into_bytes()
        || generation != state.generation().get()
        || revision != state.revision().get()
    {
        return Err(consumption_error("action marker differs from current workspace state"));
    }
    let action_id = ActionId::new(action)
        .map_err(|_| consumption_error("action marker contains an invalid action identity"))?;
    Ok((action_id, Sha256Digest::new(digest)))
}

fn take_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> [u8; N] {
    let end = *offset + N;
    let mut result = [0_u8; N];
    result.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    result
}

const fn reused_error() -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::ReceiptReused,
        WorkspaceOperation::Authorize,
        RecoveryClass::Reauthorize,
        "action receipts were already consumed by this durable workspace revision",
    )
}

const fn consumption_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Authorize,
        RecoveryClass::Quarantine,
        detail,
    )
}
