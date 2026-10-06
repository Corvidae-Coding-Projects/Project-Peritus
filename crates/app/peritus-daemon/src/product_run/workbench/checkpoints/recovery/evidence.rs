//! Versioned exact rewind recovery evidence, including typed scope and materialized postimages.

use super::{
    CheckpointFileVersion, FolderMutationActionMarker, FolderMutationRecoveryOutcome,
    FolderMutationRecoveryState, RestoreOperation, RestoreStatus, UserCheckpoint, WorkbenchCommand,
    WorkbenchIntent, WorkbenchRewindDisposition, public_version,
};
use peritus_app_protocol::{WorkbenchCheckpointFileMode, WorkbenchCheckpointVersion};

const RECOVERY_MAGIC: &[u8] = b"PERITUS-WORKBENCH-REWIND-RECOVERY-V1\0";
const RECOVERY_MAGIC_V2: &[u8] = b"PERITUS-WORKBENCH-REWIND-RECOVERY-V2\0";

#[allow(
    clippy::too_many_arguments,
    reason = "recovery evidence binds every independently checked journal and C1 fact"
)]
pub(super) fn encode_evidence(
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
    let enhanced = restore.targets().is_some()
        || preview.paths().iter().any(|path| {
            !path.ranges().is_empty()
                || matches!(path.checkpoint(), WorkbenchCheckpointVersion::EmptyDirectory { .. })
        });
    let mut bytes = if enhanced { RECOVERY_MAGIC_V2 } else { RECOVERY_MAGIC }.to_vec();
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
    if enhanced {
        put_u64(&mut bytes, restore.targets().map_or(0, <[_]>::len) as u64);
        if let Some(targets) = restore.targets() {
            for target in targets {
                put_bytes(&mut bytes, target.path().as_bytes());
                put_checkpoint_version(&mut bytes, target.checkpoint());
            }
        }
    }
    put_u64(&mut bytes, preview.paths().len() as u64);
    for (index, path) in preview.paths().iter().enumerate() {
        put_bytes(&mut bytes, path.path().as_bytes());
        bytes.push(disposition_tag(path.disposition()));
        put_public_version(&mut bytes, path.observed_current());
        put_public_version(&mut bytes, path.checkpoint());
        if enhanced {
            put_u64(&mut bytes, path.ranges().len() as u64);
            for range in path.ranges() {
                match range.selection() {
                    peritus_app_protocol::WorkbenchFileRange::All => bytes.push(0),
                    peritus_app_protocol::WorkbenchFileRange::Bytes { start, end } => {
                        bytes.push(1);
                        put_u64(&mut bytes, start);
                        put_u64(&mut bytes, end);
                    }
                    peritus_app_protocol::WorkbenchFileRange::Lines { first, last } => {
                        bytes.push(2);
                        put_u64(&mut bytes, u64::from(first));
                        put_u64(&mut bytes, u64::from(last));
                    }
                }
                let (start, end) = range.captured_interval();
                put_u64(&mut bytes, start);
                put_u64(&mut bytes, end);
            }
        }
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
        WorkbenchRewindDisposition::Unavailable => 5,
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
        WorkbenchCheckpointVersion::EmptyDirectory { permissions } => {
            bytes.push(2);
            bytes.extend_from_slice(&permissions.to_be_bytes());
        }
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
