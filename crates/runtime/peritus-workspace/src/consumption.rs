//! Durable per-revision action-consumption markers owned by the writable target.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{ErrorKind, Seek as _, SeekFrom, Write},
    path::{Path, PathBuf},
};

use peritus_types::{
    ActionId, EnvironmentId, Generation, ResourceId, RevisionNumber, Sha256Digest, WorkspaceId,
};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation, WorkspaceState, WritableWorkspace,
};

const MAGIC_V1: &[u8] = b"PERITUS-WORKSPACE-ACTION-V1\0";
const MAGIC_V2: &[u8] = b"PERITUS-WORKSPACE-ACTION-V2\0";
const HEADER_BYTES_V1: usize = MAGIC_V1.len() + 16 + 16 + 16 + 8 + 8 + 16 + 32;
const HEADER_BYTES_V2: usize = MAGIC_V2.len() + 16 + 16 + 16 + 8 + 8 + 16 + 32;
const MAX_ACTIONS_PER_REVISION: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionTerminalRecord {
    Applied { patch_identity: peritus_patch::PatchIdentity, installed_manifest: Vec<u8> },
    RolledBack,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRecord {
    pub(crate) action_digest: Sha256Digest,
    pub(crate) terminal: Option<ActionTerminalRecord>,
    pub(crate) legacy: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionConsumptionBinding {
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    generation: Generation,
    revision: RevisionNumber,
}

impl ActionConsumptionBinding {
    pub const fn new(
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Self {
        Self { workspace_id, resource_id, environment_id, generation, revision }
    }

    pub(crate) const fn from_state(state: &WorkspaceState) -> Self {
        Self::new(
            state.binding().workspace_id(),
            state.binding().resource_id(),
            state.binding().environment_id(),
            state.generation(),
            state.revision(),
        )
    }
}

impl WritableWorkspace {
    pub(crate) fn restore_action_consumption(&mut self) -> Result<(), WorkspaceError> {
        let binding = ActionConsumptionBinding::from_state(self.state());
        let actions = restore(self.transaction_root(), binding)?;
        for (action_id, action_digest) in actions {
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
        let count = self.state().consumed_action_count();
        let binding = ActionConsumptionBinding::from_state(self.state());
        commit(self.transaction_root(), binding, count, action_id, action_digest)?;
        self.state_mut().record_consumed_action(action_id, action_digest);
        Ok(())
    }
}

pub fn restore(
    transaction_root: &Path,
    binding: ActionConsumptionBinding,
) -> Result<BTreeMap<ActionId, Sha256Digest>, WorkspaceError> {
    let directory = revision_directory(transaction_root, binding);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(consumption_error("action ledger cannot be inspected")),
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(consumption_error("action ledger is not a real directory"));
    }
    let canonical = fs::canonicalize(&directory)
        .map_err(|_| consumption_error("action ledger cannot be canonicalized"))?;
    if !canonical.starts_with(transaction_root) {
        return Err(consumption_error("action ledger escaped its transaction root"));
    }
    let entries =
        fs::read_dir(&canonical).map_err(|_| consumption_error("action ledger cannot be read"))?;
    let mut count = 0_usize;
    let mut actions = BTreeMap::new();
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
        let (action_id, record, _) = decode_record(binding, &bytes)?;
        let expected_name = marker_name(action_id);
        if entry.file_name() != std::ffi::OsStr::new(&expected_name) {
            return Err(consumption_error("action marker name differs from its identity"));
        }
        if actions.insert(action_id, record.action_digest).is_some() {
            return Err(consumption_error("action ledger contains a duplicate identity"));
        }
    }
    Ok(actions)
}

pub fn commit(
    transaction_root: &Path,
    binding: ActionConsumptionBinding,
    consumed_action_count: usize,
    action_id: ActionId,
    action_digest: Sha256Digest,
) -> Result<(), WorkspaceError> {
    if consumed_action_count >= MAX_ACTIONS_PER_REVISION {
        return Err(consumption_error("action ledger exceeds its per-revision bound"));
    }
    let directory = revision_directory(transaction_root, binding);
    create_checked_directory(transaction_root, &directory)?;
    let path = directory.join(marker_name(action_id));
    let mut marker = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(marker) => marker,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(reused_error()),
        Err(_) => return Err(consumption_error("action marker cannot be created exclusively")),
    };
    let bytes = encode_header(binding, action_id, action_digest);
    marker
        .write_all(&bytes)
        .and_then(|()| marker.sync_all())
        .map_err(|_| consumption_error("action marker cannot be synchronized"))?;
    crate::filesystem::sync_directory(&directory)
        .map_err(|_| consumption_error("action ledger directory cannot be synchronized"))?;
    Ok(())
}

pub fn action_record(
    transaction_root: &Path,
    binding: ActionConsumptionBinding,
    action_id: ActionId,
) -> Result<Option<ActionRecord>, WorkspaceError> {
    let path = revision_directory(transaction_root, binding).join(marker_name(action_id));
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(consumption_error("action marker cannot be inspected")),
        Ok(_) => {}
    }
    let bytes = read_action_bytes(&path)?;
    let (actual_id, record, _) = decode_record(binding, &bytes)?;
    if actual_id != action_id {
        return Err(consumption_error("action marker differs from its file identity"));
    }
    Ok(Some(record))
}

pub fn complete_action(
    transaction_root: &Path,
    binding: ActionConsumptionBinding,
    action_id: ActionId,
    action_digest: Sha256Digest,
    terminal: &ActionTerminalRecord,
) -> Result<(), WorkspaceError> {
    let directory = revision_directory(transaction_root, binding);
    let path = directory.join(marker_name(action_id));
    let bytes = read_action_bytes(&path)?;
    let (actual_id, record, offset) = decode_record(binding, &bytes)?;
    if actual_id != action_id || record.action_digest != action_digest {
        return Err(consumption_error("action completion differs from its consumed authorization"));
    }
    if let Some(existing) = record.terminal {
        return if &existing == terminal {
            Ok(())
        } else {
            Err(consumption_error("action already has a conflicting terminal result"))
        };
    }
    let frame = encode_terminal(terminal)?;
    let mut marker = OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|_| consumption_error("action marker cannot be opened for completion"))?;
    marker
        .set_len(offset)
        .and_then(|()| marker.seek(SeekFrom::Start(offset)))
        .and_then(|_| marker.write_all(&frame))
        .and_then(|()| marker.sync_all())
        .map_err(|_| consumption_error("action completion cannot be synchronized"))?;
    crate::filesystem::sync_directory(&directory)
        .map_err(|_| consumption_error("action ledger directory cannot be synchronized"))
}

pub fn contains_action(
    actions: &BTreeMap<ActionId, Sha256Digest>,
    action_id: ActionId,
) -> Result<(), WorkspaceError> {
    if actions.contains_key(&action_id) {
        return Err(reused_error());
    }
    Ok(())
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

fn revision_directory(root: &Path, binding: ActionConsumptionBinding) -> PathBuf {
    action_ledger_root(root).join(format!(
        "generation-{}-revision-{}",
        binding.generation.get(),
        binding.revision.get()
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

fn encode_header(
    binding: ActionConsumptionBinding,
    action_id: ActionId,
    action_digest: Sha256Digest,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_BYTES_V2);
    bytes.extend_from_slice(MAGIC_V2);
    bytes.extend_from_slice(binding.workspace_id.as_bytes());
    bytes.extend_from_slice(binding.resource_id.as_bytes());
    bytes.extend_from_slice(binding.environment_id.as_bytes());
    bytes.extend_from_slice(&binding.generation.get().to_be_bytes());
    bytes.extend_from_slice(&binding.revision.get().to_be_bytes());
    bytes.extend_from_slice(action_id.as_bytes());
    bytes.extend_from_slice(action_digest.as_bytes());
    bytes
}

fn decode_record(
    binding: ActionConsumptionBinding,
    bytes: &[u8],
) -> Result<(ActionId, ActionRecord, u64), WorkspaceError> {
    let (magic, legacy, header_bytes) = if bytes.starts_with(MAGIC_V2) {
        (MAGIC_V2, false, HEADER_BYTES_V2)
    } else if bytes.starts_with(MAGIC_V1) {
        (MAGIC_V1, true, HEADER_BYTES_V1)
    } else {
        return Err(consumption_error("action marker has an unsupported format"));
    };
    if bytes.len() < header_bytes {
        return Err(consumption_error("action marker header is incomplete"));
    }
    let mut offset = magic.len();
    let workspace = take_array::<16>(bytes, &mut offset);
    let resource = take_array::<16>(bytes, &mut offset);
    let environment = take_array::<16>(bytes, &mut offset);
    let generation = u64::from_be_bytes(take_array::<8>(bytes, &mut offset));
    let revision = u64::from_be_bytes(take_array::<8>(bytes, &mut offset));
    let action = take_array::<16>(bytes, &mut offset);
    let digest = take_array::<32>(bytes, &mut offset);
    if workspace != binding.workspace_id.into_bytes()
        || resource != binding.resource_id.into_bytes()
        || environment != binding.environment_id.into_bytes()
        || generation != binding.generation.get()
        || revision != binding.revision.get()
    {
        return Err(consumption_error("action marker differs from current workspace state"));
    }
    let action_id = ActionId::new(action)
        .map_err(|_| consumption_error("action marker contains an invalid action identity"))?;
    let mut record =
        ActionRecord { action_digest: Sha256Digest::new(digest), terminal: None, legacy };
    let mut consumed = header_bytes;
    if let Some((terminal, frame_bytes)) = decode_terminal(&bytes[header_bytes..])? {
        record.terminal = Some(terminal);
        consumed = consumed
            .checked_add(frame_bytes)
            .ok_or_else(|| consumption_error("action marker length overflowed"))?;
        if decode_terminal(&bytes[consumed..])?.is_some() {
            return Err(consumption_error("action marker contains multiple terminal results"));
        }
    }
    let consumed = u64::try_from(consumed)
        .map_err(|_| consumption_error("action marker exceeds this platform"))?;
    Ok((action_id, record, consumed))
}

fn read_action_bytes(path: &Path) -> Result<Vec<u8>, WorkspaceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| consumption_error("action marker cannot be inspected"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(consumption_error("action marker is not a regular file"));
    }
    fs::read(path).map_err(|_| consumption_error("action marker cannot be read"))
}

fn encode_terminal(terminal: &ActionTerminalRecord) -> Result<Vec<u8>, WorkspaceError> {
    let mut payload = Vec::new();
    match terminal {
        ActionTerminalRecord::Applied { patch_identity, installed_manifest } => {
            payload.push(1);
            payload.extend_from_slice(patch_identity.as_bytes());
            let digest = peritus_codec::sha256(installed_manifest);
            payload.extend_from_slice(digest.as_bytes());
            let length = u64::try_from(installed_manifest.len())
                .map_err(|_| consumption_error("installed patch manifest exceeds this platform"))?;
            payload.extend_from_slice(&length.to_le_bytes());
            payload.extend_from_slice(installed_manifest);
        }
        ActionTerminalRecord::RolledBack => payload.push(2),
    }
    let length = u64::try_from(payload.len())
        .map_err(|_| consumption_error("action result exceeds this platform"))?;
    let mut frame = Vec::with_capacity(payload.len().saturating_add(40));
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(peritus_codec::sha256(&payload).as_bytes());
    Ok(frame)
}

fn decode_terminal(bytes: &[u8]) -> Result<Option<(ActionTerminalRecord, usize)>, WorkspaceError> {
    if bytes.len() < 8 {
        return Ok(None);
    }
    let length = u64::from_le_bytes(
        bytes[..8]
            .try_into()
            .map_err(|_| consumption_error("action result length is malformed"))?,
    );
    let length = usize::try_from(length)
        .map_err(|_| consumption_error("action result exceeds this platform"))?;
    let payload_end = 8_usize
        .checked_add(length)
        .ok_or_else(|| consumption_error("action result length overflowed"))?;
    let frame_end = payload_end
        .checked_add(32)
        .ok_or_else(|| consumption_error("action result length overflowed"))?;
    if frame_end > bytes.len() {
        return Ok(None);
    }
    let payload = &bytes[8..payload_end];
    if peritus_codec::sha256(payload).as_bytes() != &bytes[payload_end..frame_end] {
        return Err(consumption_error("action result checksum does not match"));
    }
    let terminal = match payload.first().copied() {
        Some(1) if payload.len() >= 73 => {
            let identity = peritus_patch::PatchIdentity::from_digest(Sha256Digest::new(
                payload[1..33]
                    .try_into()
                    .map_err(|_| consumption_error("patch identity is malformed"))?,
            ));
            let manifest_digest = &payload[33..65];
            let manifest_length = u64::from_le_bytes(
                payload[65..73]
                    .try_into()
                    .map_err(|_| consumption_error("patch manifest length is malformed"))?,
            );
            let manifest_length = usize::try_from(manifest_length)
                .map_err(|_| consumption_error("patch manifest exceeds this platform"))?;
            let end = 73_usize
                .checked_add(manifest_length)
                .ok_or_else(|| consumption_error("patch manifest length overflowed"))?;
            if end != payload.len()
                || peritus_codec::sha256(&payload[73..end]).as_bytes() != manifest_digest
            {
                return Err(consumption_error("installed patch manifest is malformed"));
            }
            ActionTerminalRecord::Applied {
                patch_identity: identity,
                installed_manifest: payload[73..end].to_vec(),
            }
        }
        Some(2) if payload.len() == 1 => ActionTerminalRecord::RolledBack,
        _ => return Err(consumption_error("action result state is unsupported")),
    };
    Ok(Some((terminal, frame_end)))
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
