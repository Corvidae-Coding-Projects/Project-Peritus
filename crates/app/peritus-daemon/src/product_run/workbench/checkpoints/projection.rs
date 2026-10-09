//! Exact checkpoint and restore projections and stable identity derivation.

use super::{
    AppErrorCode, AppProtocolError, CheckpointFileMode, CheckpointFileVersion, CheckpointId,
    CheckpointReferences, ControlError, ControlOperationId, ConversationRecord, EXTERNAL_EFFECT,
    Error, FileMode, HISTORY_EFFECT, Preimage, RestoreStatus, Sha256Digest, UserCheckpoint,
    WorkbenchCheckpointFileMode, WorkbenchCheckpointName, WorkbenchCheckpointPath,
    WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences, WorkbenchCheckpointVersion,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt, WorkbenchRestoreStatus,
};

pub(super) fn checkpoint_references(record: &ConversationRecord) -> CheckpointReferences {
    let brief_revision = record
        .brief()
        .bindings()
        .iter()
        .map(|binding| binding.selected().revision())
        .max()
        .unwrap_or(0);
    CheckpointReferences::new(
        record.revision(),
        record.inputs().generation(),
        brief_revision,
        record.goal().map(peritus_product_runner::control::GoalRecord::user_revision),
    )
}

pub(super) fn public_checkpoint(
    query: peritus_app_protocol::WorkbenchQuery,
    revision: u64,
    checkpoint: &UserCheckpoint,
) -> Result<WorkbenchCheckpointReceipt, Error> {
    let references = checkpoint.references();
    WorkbenchCheckpointReceipt::new(
        ControlOperationId::new(*checkpoint.id().as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        query,
        revision,
        WorkbenchCheckpointName::new(checkpoint.name().to_owned())
            .map_err(|_| ControlError::InvalidInput)?,
        WorkbenchCheckpointReferences::new(
            references.source_conversation_revision(),
            references.context_generation(),
            references.brief_revision(),
            references.goal_revision(),
        ),
        checkpoint
            .paths()
            .iter()
            .map(|path| {
                WorkbenchCheckpointPath::new(
                    path.path().to_owned(),
                    public_version(path.checkpoint()),
                    path.owned_postchange().map(public_version),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ControlError::InvalidInput)?,
        checkpoint.exclusions().map(str::to_owned).collect(),
        checkpoint.external_effects().map(str::to_owned).collect(),
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn public_restore(
    command: &WorkbenchCommand,
    recovery: CheckpointId,
    status: RestoreStatus,
    accepted_revision: u64,
    restored: Vec<String>,
    conflicts: Vec<String>,
) -> Result<WorkbenchRestoreReceipt, Error> {
    let WorkbenchIntent::ApplyRewind(confirmed) = command.intent() else {
        return Err(ControlError::InvalidInput.into());
    };
    WorkbenchRestoreReceipt::new(
        command.operation(),
        confirmed.request().checkpoint(),
        ControlOperationId::new(*recovery.as_bytes()).map_err(|_| ControlError::InvalidInput)?,
        command.query(),
        accepted_revision,
        match status {
            RestoreStatus::Applied => WorkbenchRestoreStatus::Applied,
            RestoreStatus::Conflict => WorkbenchRestoreStatus::Conflict,
            RestoreStatus::RecoveryRequired | RestoreStatus::Prepared => {
                WorkbenchRestoreStatus::RecoveryRequired
            }
        },
        if status == RestoreStatus::Applied { restored } else { Vec::new() },
        conflicts,
        confirmed.external_effects().to_vec(),
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) const fn public_version(value: CheckpointFileVersion) -> WorkbenchCheckpointVersion {
    match value {
        CheckpointFileVersion::Absent => WorkbenchCheckpointVersion::Absent,
        CheckpointFileVersion::Present { digest, bytes, mode } => {
            WorkbenchCheckpointVersion::Present {
                digest: Sha256Digest::new(digest),
                bytes,
                mode: match mode {
                    CheckpointFileMode::Regular => WorkbenchCheckpointFileMode::Regular,
                    CheckpointFileMode::Executable => WorkbenchCheckpointFileMode::Executable,
                },
            }
        }
    }
}

pub(super) const fn patch_preimage(value: CheckpointFileVersion) -> Preimage {
    match value {
        CheckpointFileVersion::Absent => Preimage::Absent,
        CheckpointFileVersion::Present { digest, bytes, mode } => {
            Preimage::present(Sha256Digest::new(digest), bytes, patch_mode(mode))
        }
    }
}

pub(super) const fn patch_mode(value: CheckpointFileMode) -> FileMode {
    match value {
        CheckpointFileMode::Regular => FileMode::Regular,
        CheckpointFileMode::Executable => FileMode::Executable,
    }
}

pub(super) fn external_effects() -> Vec<String> {
    vec![HISTORY_EFFECT.to_owned(), EXTERNAL_EFFECT.to_owned()]
}

pub(super) fn derived_id(domain: &[u8], source: &[u8; 16]) -> [u8; 16] {
    let mut bytes = domain.to_vec();
    bytes.extend_from_slice(source);
    digest_id(&bytes)
}

pub(super) fn digest_id(bytes: &[u8]) -> [u8; 16] {
    let digest = peritus_codec::sha256(bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    id
}

pub(super) fn noop_manifest(preview: Sha256Digest) -> Vec<u8> {
    let mut bytes = b"peritus-workbench-rewind-noop-v1\0".to_vec();
    bytes.extend_from_slice(preview.as_bytes());
    bytes
}

pub(super) fn patch_input<T>(result: Result<T, peritus_patch::PatchError>) -> Result<T, Error> {
    result.map_err(|_| ControlError::InvalidInput.into())
}

pub(super) const fn app_error(code: AppErrorCode) -> AppProtocolError {
    AppProtocolError::new(code, None)
}
