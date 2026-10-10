//! Exact checkpoint and restore projections and stable identity derivation.

use super::{
    AppErrorCode, AppProtocolError, CheckpointFileMode, CheckpointFileVersion, CheckpointId,
    CheckpointReferences, ControlError, ControlOperationId, ConversationRecord, EXTERNAL_EFFECT,
    Error, FileMode, HISTORY_EFFECT, Preimage, RestoreStatus, Sha256Digest, UserCheckpoint,
    WorkbenchCheckpointFileMode, WorkbenchCheckpointName, WorkbenchCheckpointPath,
    WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences, WorkbenchCheckpointVersion,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt, WorkbenchRestoreStatus,
    WorkbenchRestoreSummary, WorkbenchRewindDisposition, WorkbenchRewindPath,
    WorkbenchRewindPreview, WorkbenchRewindRequest,
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
        checkpoint.exclusions().collect(),
        checkpoint.external_effects().map(str::to_owned).collect(),
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

pub(in crate::product_run::workbench) enum WorkbenchRestoreProjection {
    Detailed(WorkbenchRestoreReceipt),
    Summary(WorkbenchRestoreSummary),
}

pub(super) fn public_restore(
    command: &WorkbenchCommand,
    recovery: CheckpointId,
    status: RestoreStatus,
    accepted_revision: u64,
    restored: Vec<String>,
    conflicts: Vec<String>,
    external_effects: Vec<String>,
) -> Result<WorkbenchRestoreProjection, Error> {
    let (checkpoint_id, compact_fingerprint) = match command.intent() {
        WorkbenchIntent::ApplyRewind(confirmed) => (confirmed.request().checkpoint(), None),
        WorkbenchIntent::ConfirmRewind(confirmation) => {
            (confirmation.request().checkpoint(), Some(confirmation.preview_digest()))
        }
        _ => return Err(ControlError::InvalidInput.into()),
    };
    let status = match status {
        RestoreStatus::Applied => WorkbenchRestoreStatus::Applied,
        RestoreStatus::Conflict => WorkbenchRestoreStatus::Conflict,
        RestoreStatus::RecoveryRequired | RestoreStatus::Prepared => {
            WorkbenchRestoreStatus::RecoveryRequired
        }
    };
    let recovery =
        ControlOperationId::new(*recovery.as_bytes()).map_err(|_| ControlError::InvalidInput)?;
    if let Some(fingerprint) = compact_fingerprint {
        let restored_paths = if status == WorkbenchRestoreStatus::Applied {
            u64::try_from(restored.len()).map_err(|_| ControlError::Capacity)?
        } else {
            0
        };
        let conflicting_paths = if status == WorkbenchRestoreStatus::Conflict {
            u64::try_from(conflicts.len()).map_err(|_| ControlError::Capacity)?
        } else {
            0
        };
        return WorkbenchRestoreSummary::new(
            command.operation(),
            checkpoint_id,
            recovery,
            command.query(),
            accepted_revision,
            status,
            restored_paths,
            conflicting_paths,
            fingerprint,
        )
        .map(WorkbenchRestoreProjection::Summary)
        .map_err(|_| ControlError::InvalidInput.into());
    }
    WorkbenchRestoreReceipt::new(
        command.operation(),
        checkpoint_id,
        recovery,
        command.query(),
        accepted_revision,
        status,
        if status == WorkbenchRestoreStatus::Applied { restored } else { Vec::new() },
        conflicts,
        external_effects,
    )
    .map(WorkbenchRestoreProjection::Detailed)
    .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn reconstruct_rewind_preview(
    request: WorkbenchRewindRequest,
    checkpoint: &UserCheckpoint,
    recovery: &UserCheckpoint,
) -> Result<WorkbenchRewindPreview, Error> {
    let paths = if request.mode() == peritus_app_protocol::WorkbenchRewindMode::ConversationOnly {
        if !recovery.paths().is_empty() {
            return Err(Error::Corrupt("conversation-only restore retained file coverage"));
        }
        Vec::new()
    } else {
        if checkpoint.paths().len() != recovery.paths().len() {
            return Err(Error::Corrupt("restore checkpoint coverage differs from recovery"));
        }
        checkpoint
            .paths()
            .iter()
            .zip(recovery.paths())
            .map(|(target, before)| {
                if target.path() != before.path() {
                    return Err(Error::Corrupt("restore checkpoint path order changed"));
                }
                let disposition = if target.checkpoint() == before.checkpoint() {
                    WorkbenchRewindDisposition::Unchanged
                } else if target.owned_postchange().is_none() {
                    WorkbenchRewindDisposition::Unsealed
                } else if target.owned_postchange() == Some(before.checkpoint()) {
                    WorkbenchRewindDisposition::Restore
                } else {
                    WorkbenchRewindDisposition::Conflict
                };
                WorkbenchRewindPath::new(
                    target.path().to_owned(),
                    public_version(target.checkpoint()),
                    target.owned_postchange().map(public_version),
                    public_version(before.checkpoint()),
                    disposition,
                )
                .map_err(|_| ControlError::InvalidInput.into())
            })
            .collect::<Result<Vec<_>, Error>>()?
    };
    let exclusions =
        if request.mode() == peritus_app_protocol::WorkbenchRewindMode::ConversationOnly {
            vec![
                "Current files are unchanged; this branch is not a historical filesystem snapshot."
                    .to_owned(),
            ]
        } else {
            checkpoint.exclusions().collect()
        };
    WorkbenchRewindPreview::new(
        request,
        paths,
        exclusions,
        checkpoint.external_effects().map(str::to_owned).collect(),
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
    result.map_err(Error::from)
}

pub(super) const fn app_error(code: AppErrorCode) -> AppProtocolError {
    AppProtocolError::new(code, None)
}
