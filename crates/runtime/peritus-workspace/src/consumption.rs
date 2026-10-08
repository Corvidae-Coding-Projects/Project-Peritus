//! Durable per-revision action-consumption markers owned by the writable target.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{ErrorKind, Seek as _, SeekFrom, Write},
    path::{Path, PathBuf},
};

use peritus_artifact_store::ArtifactDigest;
use peritus_git::CommitId;
use peritus_types::{
    ActionId, EnvironmentId, EventId, Generation, ResourceId, RevisionNumber, Sha256Digest,
    SnapshotId, WorkspaceId,
};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation, WorkspaceState, WritableWorkspace,
};

const MAX_ACTIONS_PER_REVISION: usize = 1_024;

mod codec;
use codec::{decode_record, encode_header, encode_plan, encode_terminal, read_action_bytes};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionTerminalRecord {
    Applied {
        patch_identity: peritus_patch::PatchIdentity,
        installed_manifest: Vec<u8>,
    },
    RolledBack,
    Candidate {
        patch_identity: peritus_patch::PatchIdentity,
        detail_digest: Sha256Digest,
        artifact_digest: ArtifactDigest,
        artifact_size: u64,
        snapshot_manifest: Vec<u8>,
        workspace_manifest: Vec<u8>,
    },
    WorkspaceRollback {
        restored_from: CommitId,
        detail_digest: Sha256Digest,
        artifact_digest: ArtifactDigest,
        artifact_size: u64,
        snapshot_manifest: Vec<u8>,
        workspace_manifest: Vec<u8>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRecord {
    pub(crate) action_digest: Sha256Digest,
    /// Exact planned Git successor identity and payload digest, persisted before Git effects.
    pub(crate) plan: Option<ActionPlan>,
    pub(crate) terminal: Option<ActionTerminalRecord>,
    pub(crate) legacy: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionPlan {
    pub operation: u8,
    pub snapshot_id: SnapshotId,
    pub payload_digest: Sha256Digest,
    pub installed_revision: RevisionNumber,
    pub dispatch_event: EventId,
    pub patch_identity: Option<peritus_patch::PatchIdentity>,
    pub patch_manifest_digest: Option<Sha256Digest>,
    pub target_snapshot_id: Option<SnapshotId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Exact prior-revision key for one durable workspace action marker.
pub struct ActionConsumptionBinding {
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    generation: Generation,
    revision: RevisionNumber,
}

impl ActionConsumptionBinding {
    /// Creates a binding from the exact workspace lineage and logical revision.
    #[must_use]
    pub const fn new(
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Self {
        Self { workspace_id, resource_id, environment_id, generation, revision }
    }

    /// Workspace lineage selected by this marker.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId {
        self.workspace_id
    }
    /// Resource identity selected by this marker.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
    /// Environment identity selected by this marker.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }
    /// Workspace generation selected by this marker.
    #[must_use]
    pub const fn generation(self) -> Generation {
        self.generation
    }
    /// Prior logical revision selected by this marker.
    #[must_use]
    pub const fn revision(self) -> RevisionNumber {
        self.revision
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

    pub(crate) fn prepare_action_consumption(
        &self,
        action_id: ActionId,
        action_digest: Sha256Digest,
        plan: &ActionPlan,
    ) -> Result<(), WorkspaceError> {
        prepare_action(
            self.transaction_root(),
            ActionConsumptionBinding::from_state(self.state()),
            action_id,
            action_digest,
            plan,
        )
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

/// Persists the exact intended Git successor before creating or restoring Git objects.
pub fn prepare_action(
    transaction_root: &Path,
    binding: ActionConsumptionBinding,
    action_id: ActionId,
    action_digest: Sha256Digest,
    plan: &ActionPlan,
) -> Result<(), WorkspaceError> {
    let path = revision_directory(transaction_root, binding).join(marker_name(action_id));
    let bytes = read_action_bytes(&path)?;
    let (actual_id, record, offset) = decode_record(binding, &bytes)?;
    if actual_id != action_id || record.action_digest != action_digest {
        return Err(consumption_error("action plan differs from its consumed authorization"));
    }
    if let Some(existing) = record.plan.as_ref() {
        return if existing == plan {
            Ok(())
        } else {
            Err(consumption_error("action already has a conflicting plan"))
        };
    }
    if record.terminal.is_some() {
        return Err(consumption_error("completed action cannot receive a new plan"));
    }
    let frame = encode_plan(plan);
    let mut marker = OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|_| consumption_error("action marker cannot be opened for planning"))?;
    marker
        .set_len(offset)
        .and_then(|()| marker.seek(SeekFrom::Start(offset)))
        .and_then(|_| marker.write_all(&frame))
        .and_then(|()| marker.sync_all())
        .map_err(|_| consumption_error("action plan cannot be synchronized"))?;
    crate::filesystem::sync_directory(path.parent().expect("marker has parent"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_plan_round_trips_with_expected_revision_before_terminal() {
        let binding = ActionConsumptionBinding::new(
            WorkspaceId::new([1; 16]).expect("workspace"),
            ResourceId::new([2; 16]).expect("resource"),
            EnvironmentId::new([3; 16]).expect("environment"),
            Generation::first(),
            RevisionNumber::first(),
        );
        let action = ActionId::new([4; 16]).expect("action");
        let digest = Sha256Digest::new([5; 32]);
        let plan = ActionPlan {
            operation: 3,
            snapshot_id: SnapshotId::new([6; 16]).expect("snapshot"),
            payload_digest: Sha256Digest::new([7; 32]),
            installed_revision: RevisionNumber::new(2).expect("installed revision"),
            dispatch_event: EventId::new([8; 16]).expect("event"),
            patch_identity: Some(peritus_patch::PatchIdentity::from_digest(Sha256Digest::new(
                [9; 32],
            ))),
            patch_manifest_digest: Some(Sha256Digest::new([10; 32])),
            target_snapshot_id: None,
        };
        let mut bytes = encode_header(binding, action, digest);
        bytes.extend_from_slice(&encode_plan(&plan));

        let (actual_action, record, offset) = decode_record(binding, &bytes).expect("decode plan");

        assert_eq!(actual_action, action);
        assert_eq!(record.action_digest, digest);
        assert_eq!(record.plan, Some(plan));
        assert_eq!(record.terminal, None);
        assert_eq!(usize::try_from(offset).expect("marker offset"), bytes.len());
    }
}
