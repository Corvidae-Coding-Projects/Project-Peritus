//! Restart recovery and conservative rollback.

use std::{fs, io, path::Path};

use crate::{
    ErrorCode, PatchError, PatchOperationContext, PatchOperationKind, RecoveryClass, RollbackStatus,
};

use super::{
    RecoveryBinding, RecoveryOutcome, RecoveryState,
    filesystem::{
        Observation, checked_target_path, observation_matches, observe_absolute, observe_target,
        remove_created_directories, sync_directory,
    },
    manifest::{Manifest, ManifestEntry, TransactionPhase},
    recovery_observation::observe_manifest,
    roots::recovery_roots,
    storage::{MANIFEST_FILE, backup_path, cleanup_transaction, persist_manifest},
};

/// Inspects and safely resolves one restart-visible transaction directory.
///
/// `expected_binding` must exactly match the workspace identity, generation, and revision encoded
/// by the manifest. A mismatch returns an indeterminate outcome carrying the observed binding and
/// performs no workspace or transaction mutation.
///
/// # Errors
///
/// Returns an I/O or root-safety error when the transaction cannot even be inspected. Malformed
/// manifests are quarantined and reported as [`RecoveryState::Indeterminate`].
pub fn recover_transaction(
    workspace_root: impl AsRef<Path>,
    transaction_directory: impl AsRef<Path>,
    expected_binding: RecoveryBinding,
) -> Result<RecoveryOutcome, PatchError> {
    let original_transaction_directory = transaction_directory.as_ref();
    let (workspace, transaction_directory) =
        recovery_roots(workspace_root.as_ref(), original_transaction_directory)?;
    let bytes = match read_manifest(&transaction_directory) {
        Ok(bytes) => bytes,
        Err(error)
            if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::InvalidData) =>
        {
            let quarantined = quarantine(&transaction_directory)?;
            return Ok(RecoveryOutcome::new(
                RecoveryState::Indeterminate,
                None,
                None,
                quarantined,
                false,
            ));
        }
        Err(error) => {
            return Err(PatchError::io(
                PatchOperationContext::Recover,
                RollbackStatus::Indeterminate,
                error,
            ));
        }
    };
    let Ok(manifest) = Manifest::decode(&bytes) else {
        let quarantined = quarantine(&transaction_directory)?;
        return Ok(RecoveryOutcome::new(
            RecoveryState::Indeterminate,
            None,
            None,
            quarantined,
            false,
        ));
    };
    let observed_binding = manifest.binding();
    if observed_binding != expected_binding {
        return Ok(RecoveryOutcome::new(
            RecoveryState::Indeterminate,
            Some(observed_binding),
            Some(manifest.identity),
            false,
            false,
        ));
    }
    if transaction_directory.file_name().and_then(|name| name.to_str())
        != Some(format!("txn-{}", manifest.identity.to_hex()).as_str())
    {
        let quarantined = quarantine(&transaction_directory)?;
        return Ok(RecoveryOutcome::new(
            RecoveryState::Indeterminate,
            Some(observed_binding),
            Some(manifest.identity),
            quarantined,
            false,
        ));
    }

    classify_transaction(&workspace, &transaction_directory, manifest)
}

fn classify_transaction(
    workspace: &Path,
    transaction_directory: &Path,
    mut manifest: Manifest,
) -> Result<RecoveryOutcome, PatchError> {
    let binding = manifest.binding();
    let Some(facts) = observe_manifest(workspace, transaction_directory, &manifest)? else {
        return Ok(indeterminate(binding, manifest.identity));
    };

    match manifest.phase {
        TransactionPhase::Prepared if facts.all_pre => completed_outcome(
            RecoveryState::RolledBackCleanly,
            binding,
            manifest.identity,
            transaction_directory,
        ),
        TransactionPhase::Installing if facts.all_pre => {
            if rollback_workspace(workspace, transaction_directory, &manifest).is_err() {
                return Ok(indeterminate(binding, manifest.identity));
            }
            completed_outcome(
                RecoveryState::RolledBackCleanly,
                binding,
                manifest.identity,
                transaction_directory,
            )
        }
        TransactionPhase::Installing if facts.all_post => {
            manifest.phase = TransactionPhase::Installed;
            persist_manifest(transaction_directory, &manifest.encode()?)?;
            completed_outcome(
                RecoveryState::AlreadyApplied,
                binding,
                manifest.identity,
                transaction_directory,
            )
        }
        TransactionPhase::Installing if facts.all_recoverable => {
            if rollback_workspace(workspace, transaction_directory, &manifest).is_err() {
                return Ok(indeterminate(binding, manifest.identity));
            }
            completed_outcome(
                RecoveryState::RolledBackCleanly,
                binding,
                manifest.identity,
                transaction_directory,
            )
        }
        TransactionPhase::Installed if facts.all_post => completed_outcome(
            RecoveryState::AlreadyApplied,
            binding,
            manifest.identity,
            transaction_directory,
        ),
        _ => Ok(RecoveryOutcome::new(
            RecoveryState::Dirty,
            Some(binding),
            Some(manifest.identity),
            false,
            false,
        )),
    }
}

const fn indeterminate(
    binding: RecoveryBinding,
    identity: crate::PatchIdentity,
) -> RecoveryOutcome {
    RecoveryOutcome::new(RecoveryState::Indeterminate, Some(binding), Some(identity), false, false)
}

pub(super) fn rollback_workspace(
    workspace: &Path,
    transaction_directory: &Path,
    manifest: &Manifest,
) -> Result<(), PatchError> {
    for (index, entry) in manifest.entries.iter().enumerate().rev() {
        rollback_entry(workspace, transaction_directory, index, entry)?;
    }
    for entry in &manifest.entries {
        let observed = observe_target(
            workspace,
            &entry.path,
            PatchOperationContext::Rollback,
            RollbackStatus::Indeterminate,
        )?;
        if !observation_matches(observed, entry.preimage) {
            return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
        }
    }
    remove_created_directories(workspace, &manifest.created_directories)
}

fn rollback_entry(
    workspace: &Path,
    transaction_directory: &Path,
    index: usize,
    entry: &ManifestEntry,
) -> Result<(), PatchError> {
    let target = checked_target_path(
        workspace,
        &entry.path,
        PatchOperationContext::Rollback,
        RollbackStatus::Indeterminate,
    )?;
    let parent = target
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Rollback))?;
    let observed = observe_target(
        workspace,
        &entry.path,
        PatchOperationContext::Rollback,
        RollbackStatus::Indeterminate,
    )?;
    let target_is_pre = observation_matches(observed, entry.preimage);
    let target_is_post = observation_matches(observed, entry.postimage);

    if entry.kind == PatchOperationKind::Create {
        if target_is_post {
            fs::remove_file(&target).map_err(rollback_io)?;
            sync_directory(parent, RollbackStatus::Indeterminate)?;
        } else if !target_is_pre {
            return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
        }
        let backup = backup_path(transaction_directory, index);
        if observe_absolute(
            &backup,
            PatchOperationContext::Rollback,
            RollbackStatus::Indeterminate,
        )? != Observation::Absent
        {
            return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
        }
        return Ok(());
    }

    if target_is_post && !target_is_pre {
        if observed != Observation::Absent {
            fs::remove_file(&target).map_err(rollback_io)?;
            sync_directory(parent, RollbackStatus::Indeterminate)?;
        }
    } else if !(target_is_pre
        || entry.kind == PatchOperationKind::Replace && observed == Observation::Absent)
    {
        return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
    }

    let current = observe_target(
        workspace,
        &entry.path,
        PatchOperationContext::Rollback,
        RollbackStatus::Indeterminate,
    )?;
    let backup = backup_path(transaction_directory, index);
    let backup_observed =
        observe_absolute(&backup, PatchOperationContext::Rollback, RollbackStatus::Indeterminate)?;
    let backup_is_pre = observation_matches(backup_observed, entry.preimage);
    if observation_matches(current, entry.preimage) {
        if backup_observed != Observation::Absent {
            if !backup_is_pre {
                return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
            }
            fs::remove_file(&backup).map_err(rollback_io)?;
            sync_directory(transaction_directory, RollbackStatus::Indeterminate)?;
        }
    } else if current == Observation::Absent && backup_is_pre {
        fs::rename(&backup, &target).map_err(rollback_io)?;
        sync_directory(parent, RollbackStatus::Indeterminate)?;
        sync_directory(transaction_directory, RollbackStatus::Indeterminate)?;
    } else {
        return Err(PatchError::indeterminate(PatchOperationContext::Rollback));
    }
    Ok(())
}

fn completed_outcome(
    state: RecoveryState,
    binding: RecoveryBinding,
    identity: crate::PatchIdentity,
    transaction_directory: &Path,
) -> Result<RecoveryOutcome, PatchError> {
    let parent = transaction_directory
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Cleanup))?;
    let cleanup_pending = cleanup_transaction(transaction_directory, parent).is_err();
    Ok(RecoveryOutcome::new(state, Some(binding), Some(identity), false, cleanup_pending))
}

fn read_manifest(transaction_directory: &Path) -> io::Result<Vec<u8>> {
    let path = transaction_directory.join(MANIFEST_FILE);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > peritus_codec::CodecLimits::PRODUCTION.max_payload_bytes as u64
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unsafe manifest file"));
    }
    fs::read(path)
}

fn quarantine(transaction_directory: &Path) -> Result<bool, PatchError> {
    let parent = transaction_directory
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Recover))?;
    let name = transaction_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Recover))?;
    for suffix in 0..100u8 {
        let quarantine_name = if suffix == 0 {
            format!("{name}.quarantine")
        } else {
            format!("{name}.quarantine-{suffix}")
        };
        let destination = parent.join(quarantine_name);
        match fs::rename(transaction_directory, &destination) {
            Ok(()) => {
                sync_directory(parent, RollbackStatus::Indeterminate)?;
                return Ok(true);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(PatchError::io(
                    PatchOperationContext::Recover,
                    RollbackStatus::Indeterminate,
                    error,
                ));
            }
        }
    }
    Err(PatchError::message(
        ErrorCode::CorruptManifest,
        RecoveryClass::FenceWorkspace,
        PatchOperationContext::Recover,
        RollbackStatus::Indeterminate,
        "no bounded quarantine name was available",
    ))
}

fn rollback_io(error: io::Error) -> PatchError {
    PatchError::io(PatchOperationContext::Rollback, RollbackStatus::Indeterminate, error)
}
