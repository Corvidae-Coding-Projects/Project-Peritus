//! Prepared rewind reconciliation against exact C1 and retained checkpoint evidence.

use peritus_app_protocol::{
    WorkbenchCheckpointFileMode, WorkbenchCheckpointVersion, WorkbenchCommand, WorkbenchIntent,
    WorkbenchRewindDisposition, WorkbenchRewindMode,
};
use peritus_product_runner::control::{
    CheckpointFileVersion, ConversationRecord, RestoreOperation, RestoreStatus, UserCheckpoint,
};
use peritus_types::{ActionId, ActorId};
use peritus_workspace::{
    FolderMutationActionMarker, FolderMutationRecoveryOutcome, FolderMutationRecoveryRequest,
    FolderMutationRecoveryState, recover_folder_mutation,
};

use super::{ControlError, ControlStore, Error, ProductRunService, public_version};

const RECOVERY_MAGIC: &[u8] = b"PERITUS-WORKBENCH-REWIND-RECOVERY-V1\0";

pub(super) struct RecoveredRestore {
    pub(super) status: RestoreStatus,
    pub(super) conflicts: Vec<String>,
    pub(super) evidence: Vec<u8>,
}

impl ProductRunService {
    pub(super) fn recover_prepared_restore(
        &self,
        store: &ControlStore,
        record: &ConversationRecord,
        actor: ActorId,
        command: &WorkbenchCommand,
        restore: &RestoreOperation,
        recovery: &UserCheckpoint,
    ) -> Result<RecoveredRestore, Error> {
        let WorkbenchIntent::ApplyRewind(confirmed) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        let checkpoint = record
            .checkpoints()
            .iter()
            .find(|candidate| candidate.id() == restore.checkpoint())
            .ok_or(Error::Corrupt("restore source checkpoint missing"))?;
        let branch_exact = match (confirmed.request().child(), restore.branch()) {
            (None, None) => true,
            (Some(child), Some(branch)) => child.as_bytes() == branch.child().as_bytes(),
            (None, Some(_)) | (Some(_), None) => false,
        };
        let structurally_exact =
            branch_exact && exact_restore_inputs(checkpoint, recovery, confirmed);
        let original_conflicts = confirmed
            .paths()
            .iter()
            .filter(|path| {
                matches!(
                    path.disposition(),
                    WorkbenchRewindDisposition::Conflict | WorkbenchRewindDisposition::Unsealed
                )
            })
            .map(|path| path.path().to_owned())
            .collect::<Vec<_>>();
        let restored = confirmed
            .paths()
            .iter()
            .filter(|path| path.disposition() == WorkbenchRewindDisposition::Restore)
            .map(|path| path.path().to_owned())
            .collect::<Vec<_>>();

        let (plan_reconstructed, plan) = if structurally_exact && original_conflicts.is_empty() {
            match self.restore_plan_with_store(store, command.query(), checkpoint, confirmed) {
                Ok(plan) => (true, plan),
                Err(_) => (false, None),
            }
        } else {
            (false, None)
        };
        let plan_exact = if !structurally_exact {
            false
        } else if !original_conflicts.is_empty() {
            restore.patch_digest() == confirmed.preview_digest()
        } else {
            plan_reconstructed
                && match &plan {
                    Some(patch) => patch.identity().digest() == restore.patch_digest(),
                    None => {
                        restored.is_empty() && restore.patch_digest() == confirmed.preview_digest()
                    }
                }
        };

        let mut c1 = None;
        if structurally_exact
            && plan_exact
            && let Some(patch) = plan
        {
            let root = self.workspace_root(command.query())?;
            if let Ok(identity) = self.checked_folder_identity(command.query(), root)
                && let (Ok(resource), Ok(environment), Ok(action)) = (
                    super::super::folder_mutation::folder_resource_id(command.query().workspace()),
                    super::super::folder_mutation::folder_environment_id(&identity),
                    ActionId::new(command.operation().into_bytes())
                        .map_err(|_| ControlError::InvalidInput),
                )
            {
                c1 = recover_folder_mutation(FolderMutationRecoveryRequest::new(
                    identity,
                    resource,
                    environment,
                    actor,
                    action,
                    self.inner.directory.join("workbench-folder-transactions"),
                    patch,
                ))
                .ok();
            }
        }

        let observed = self
            .capture_checkpoint_paths(record, command.query(), recovery)
            .ok()
            .filter(|paths| paths.len() == confirmed.paths().len());
        let versions = observed.as_ref().map(|paths| {
            paths.iter().map(|path| path.version).collect::<Vec<CheckpointFileVersion>>()
        });

        let (all_pre, all_post) = classify_restore_paths(confirmed, recovery, versions.as_deref());
        let (status, conflicts) = if !structurally_exact || !plan_exact {
            (RestoreStatus::RecoveryRequired, Vec::new())
        } else if !original_conflicts.is_empty() {
            // A conflicting preview never enters C1. The prepared journal itself proves that the
            // branch was selected before any patch plan could be authorized.
            (RestoreStatus::Conflict, original_conflicts)
        } else if restored.is_empty() {
            if all_pre && all_post {
                (RestoreStatus::Applied, Vec::new())
            } else {
                (RestoreStatus::RecoveryRequired, Vec::new())
            }
        } else {
            classify_c1(c1.as_ref(), all_pre, all_post, restored)
        };
        let evidence = encode_evidence(
            command,
            restore,
            recovery,
            status,
            structurally_exact,
            plan_exact,
            versions.as_deref(),
            c1.as_ref(),
        );
        Ok(RecoveredRestore { status, conflicts, evidence })
    }
}

fn exact_restore_inputs(
    checkpoint: &UserCheckpoint,
    recovery: &UserCheckpoint,
    preview: &peritus_app_protocol::WorkbenchRewindPreview,
) -> bool {
    if preview.request().mode() == WorkbenchRewindMode::ConversationOnly {
        return preview.paths().is_empty() && recovery.paths().is_empty();
    }
    checkpoint.paths().len() == preview.paths().len()
        && recovery.paths().len() == preview.paths().len()
        && checkpoint.paths().iter().zip(recovery.paths()).zip(preview.paths()).all(
            |((target, before), shown)| {
                target.path() == shown.path()
                    && before.path() == shown.path()
                    && public_version(target.checkpoint()) == shown.checkpoint()
                    && target.owned_postchange().map(public_version) == shown.expected_current()
                    && public_version(before.checkpoint()) == shown.observed_current()
            },
        )
}

fn classify_restore_paths(
    preview: &peritus_app_protocol::WorkbenchRewindPreview,
    recovery: &UserCheckpoint,
    observed: Option<&[CheckpointFileVersion]>,
) -> (bool, bool) {
    let Some(observed) = observed else { return (false, false) };
    let mut all_pre = true;
    let mut all_post = true;
    for ((shown, before), current) in preview.paths().iter().zip(recovery.paths()).zip(observed) {
        all_pre &= *current == before.checkpoint();
        all_post &= public_version(*current) == shown.checkpoint();
    }
    (all_pre, all_post)
}

fn classify_c1(
    outcome: Option<&FolderMutationRecoveryOutcome>,
    all_pre: bool,
    all_post: bool,
    restored: Vec<String>,
) -> (RestoreStatus, Vec<String>) {
    let Some(outcome) = outcome else {
        return (RestoreStatus::RecoveryRequired, Vec::new());
    };
    let conclusive = !outcome.cleanup_pending()
        && !outcome.quarantined()
        && outcome.marker() == FolderMutationActionMarker::Exact;
    let state = outcome.state();
    if state == FolderMutationRecoveryState::NoAttempt && all_pre {
        // NoAttempt necessarily carries a missing marker, so this remains a safe no-effect
        // conflict instead of claiming that C1 applied anything.
        return (RestoreStatus::Conflict, restored);
    }
    if conclusive
        && all_post
        && matches!(
            state,
            FolderMutationRecoveryState::ConsumedWithoutTransaction
                | FolderMutationRecoveryState::AlreadyApplied
        )
    {
        return (RestoreStatus::Applied, Vec::new());
    }
    if conclusive
        && all_pre
        && matches!(
            state,
            FolderMutationRecoveryState::ConsumedWithoutTransaction
                | FolderMutationRecoveryState::RolledBackCleanly
        )
    {
        return (RestoreStatus::Conflict, restored);
    }
    (RestoreStatus::RecoveryRequired, Vec::new())
}

#[allow(
    clippy::too_many_arguments,
    reason = "recovery evidence binds every independently checked journal and C1 fact"
)]
fn encode_evidence(
    command: &WorkbenchCommand,
    restore: &RestoreOperation,
    recovery: &UserCheckpoint,
    status: RestoreStatus,
    structurally_exact: bool,
    plan_exact: bool,
    observed: Option<&[CheckpointFileVersion]>,
    c1: Option<&FolderMutationRecoveryOutcome>,
) -> Vec<u8> {
    let WorkbenchIntent::ApplyRewind(preview) = command.intent() else {
        return RECOVERY_MAGIC.to_vec();
    };
    let mut bytes = RECOVERY_MAGIC.to_vec();
    bytes.extend_from_slice(command.operation().as_bytes());
    bytes.extend_from_slice(command.query().workspace().as_bytes());
    bytes.extend_from_slice(restore.id().as_bytes());
    bytes.extend_from_slice(restore.checkpoint().as_bytes());
    bytes.extend_from_slice(recovery.id().as_bytes());
    bytes.extend_from_slice(restore.preview_digest().as_bytes());
    bytes.extend_from_slice(restore.patch_digest().as_bytes());
    bytes.push(status_tag(status));
    bytes.push(u8::from(structurally_exact));
    bytes.push(u8::from(plan_exact));
    put_u64(&mut bytes, preview.paths().len() as u64);
    for (index, path) in preview.paths().iter().enumerate() {
        put_bytes(&mut bytes, path.path().as_bytes());
        bytes.push(disposition_tag(path.disposition()));
        put_public_version(&mut bytes, path.observed_current());
        put_public_version(&mut bytes, path.checkpoint());
        match observed.and_then(|versions| versions.get(index)).copied() {
            Some(version) => {
                bytes.push(1);
                put_checkpoint_version(&mut bytes, version);
            }
            None => bytes.push(0),
        }
    }
    match c1 {
        None => bytes.push(0),
        Some(outcome) => {
            bytes.push(1);
            bytes.push(c1_state_tag(outcome.state()));
            bytes.push(marker_tag(outcome.marker()));
            bytes.extend_from_slice(outcome.action_id().as_bytes());
            bytes.extend_from_slice(outcome.action_digest().as_bytes());
            bytes.extend_from_slice(outcome.workspace_id().as_bytes());
            bytes.extend_from_slice(outcome.resource_id().as_bytes());
            put_u64(&mut bytes, outcome.generation().get());
            put_u64(&mut bytes, outcome.revision().get());
            bytes.extend_from_slice(outcome.patch_identity().as_bytes());
            match outcome.observed_binding() {
                None => bytes.push(0),
                Some(binding) => {
                    bytes.push(1);
                    bytes.extend_from_slice(binding.workspace_id().as_bytes());
                    put_u64(&mut bytes, binding.generation().get());
                    put_u64(&mut bytes, binding.revision().get());
                }
            }
            match outcome.observed_identity() {
                None => bytes.push(0),
                Some(identity) => {
                    bytes.push(1);
                    bytes.extend_from_slice(identity.as_bytes());
                }
            }
            bytes.push(u8::from(outcome.quarantined()));
            bytes.push(u8::from(outcome.cleanup_pending()));
        }
    }
    bytes
}

const fn status_tag(value: RestoreStatus) -> u8 {
    match value {
        RestoreStatus::Prepared => 0,
        RestoreStatus::Applied => 1,
        RestoreStatus::Conflict => 2,
        RestoreStatus::RecoveryRequired => 3,
    }
}

const fn disposition_tag(value: WorkbenchRewindDisposition) -> u8 {
    match value {
        WorkbenchRewindDisposition::Restore => 1,
        WorkbenchRewindDisposition::Unchanged => 2,
        WorkbenchRewindDisposition::Conflict => 3,
        WorkbenchRewindDisposition::Unsealed => 4,
    }
}

const fn c1_state_tag(value: FolderMutationRecoveryState) -> u8 {
    match value {
        FolderMutationRecoveryState::NoAttempt => 1,
        FolderMutationRecoveryState::ConsumedWithoutTransaction => 2,
        FolderMutationRecoveryState::AlreadyApplied => 3,
        FolderMutationRecoveryState::RolledBackCleanly => 4,
        FolderMutationRecoveryState::Dirty => 5,
        FolderMutationRecoveryState::Indeterminate => 6,
    }
}

const fn marker_tag(value: FolderMutationActionMarker) -> u8 {
    match value {
        FolderMutationActionMarker::Missing => 1,
        FolderMutationActionMarker::Exact => 2,
        FolderMutationActionMarker::DigestMismatch => 3,
    }
}

fn put_checkpoint_version(bytes: &mut Vec<u8>, value: CheckpointFileVersion) {
    put_public_version(bytes, public_version(value));
}

fn put_public_version(bytes: &mut Vec<u8>, value: WorkbenchCheckpointVersion) {
    match value {
        WorkbenchCheckpointVersion::Absent => bytes.push(0),
        WorkbenchCheckpointVersion::Present { digest, bytes: size, mode } => {
            bytes.push(1);
            bytes.extend_from_slice(digest.as_bytes());
            put_u64(bytes, size);
            bytes.push(match mode {
                WorkbenchCheckpointFileMode::Regular => 1,
                WorkbenchCheckpointFileMode::Executable => 2,
            });
        }
    }
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    put_u64(output, value.len() as u64);
    output.extend_from_slice(value);
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
mod tests;
