//! Restart recovery and conservative rollback.

mod storage;
use storage::{quarantine, read_manifest};

use std::{fs, io, path::Path};

use crate::{PatchError, PatchOperationContext, PatchOperationKind, RollbackStatus};

use super::{
    RecoveryBinding, RecoveryOutcome, RecoveryState,
    filesystem::{
        Observation, checked_target_path, observation_matches, observe_absolute, observe_target,
        remove_created_directories, sync_directory,
    },
    manifest::{Manifest, ManifestEntry, TransactionPhase},
    recovery_observation::observe_manifest,
    roots::{prepare_roots, recovery_roots},
    storage::{MANIFEST_FILE, backup_path, cleanup_transaction, persist_manifest},
};

/// Idempotently removes transaction files after an exact installed result was durably retained.
///
/// A missing transaction directory is treated as completed cleanup and its parent is
/// synchronized again. Existing entries must be known transaction leaves; foreign entries are
/// preserved and reported.
///
/// # Errors
///
/// Returns an error if workspace roots, the installed manifest, or retained transaction entries
/// cannot be proven safe, or if cleanup or directory synchronization fails.
pub fn cleanup_applied_transaction(
    workspace_root: impl AsRef<Path>,
    transaction_directory: impl AsRef<Path>,
    applied: &super::AppliedPatch,
) -> Result<(), PatchError> {
    let requested = transaction_directory.as_ref();
    let parent = requested
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Cleanup))?;
    let name = requested
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Cleanup))?;
    let expected_name = format!("txn-{}", applied.identity().to_hex());
    if name != expected_name {
        return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
    }
    let roots = prepare_roots(workspace_root.as_ref(), parent)?;
    let directory = roots.transaction_root.join(name);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return sync_directory(&roots.transaction_root, RollbackStatus::NotRequired);
        }
        Err(error) => return Err(rollback_io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
    }
    let manifest = Manifest::decode(applied.installed_manifest())?;
    if manifest.identity != applied.identity() || manifest.phase != TransactionPhase::Installed {
        return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
    }
    let mut allowed =
        std::collections::BTreeSet::from([MANIFEST_FILE.to_owned(), "manifest.next".to_owned()]);
    for (index, entry) in manifest.entries.iter().enumerate() {
        if entry.postimage.is_some() {
            allowed.insert(format!("final-{index:04}"));
        }
        if entry.kind != PatchOperationKind::Create {
            allowed.insert(format!("backup-{index:04}"));
        }
    }
    let mut leaves = Vec::new();
    for entry in fs::read_dir(&directory).map_err(rollback_io)? {
        let entry = entry.map_err(rollback_io)?;
        let name = entry.file_name();
        let Some(name_text) = name.to_str() else {
            return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
        };
        if !allowed.contains(name_text) {
            return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(rollback_io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
        }
        if name_text == MANIFEST_FILE
            && fs::read(entry.path()).map_err(rollback_io)? != applied.installed_manifest()
        {
            return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
        }
        if name_text == "manifest.next" {
            return Err(PatchError::indeterminate(PatchOperationContext::Cleanup));
        }
        leaves.push(entry.path());
    }
    for leaf in leaves {
        match fs::remove_file(leaf) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(rollback_io(error)),
        }
    }
    match fs::remove_dir(&directory) {
        Ok(()) => sync_directory(&roots.transaction_root, RollbackStatus::NotRequired),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            sync_directory(&roots.transaction_root, RollbackStatus::NotRequired)
        }
        Err(error) => Err(rollback_io(error)),
    }
}

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
    recover_transaction_inner(
        workspace_root.as_ref(),
        transaction_directory.as_ref(),
        expected_binding,
        None,
        |_| Ok(()),
    )
}

/// Resolves one transaction and persists its terminal result before transaction cleanup.
///
/// When recovery proves the patch was installed, the callback receives the exact durable
/// [`super::AppliedPatch`]. A verified rollback is represented by `None`. Dirty and indeterminate
/// observations do not invoke the callback.
///
/// # Errors
///
/// Returns an error if transaction roots cannot be safely inspected or the terminal completion
/// callback cannot be durably recorded.
pub fn recover_transaction_with_completion(
    workspace_root: impl AsRef<Path>,
    transaction_directory: impl AsRef<Path>,
    expected_binding: RecoveryBinding,
    expected_identity: crate::PatchIdentity,
    completion: impl FnOnce(Option<&super::AppliedPatch>) -> Result<(), PatchError>,
) -> Result<RecoveryOutcome, PatchError> {
    recover_transaction_inner(
        workspace_root.as_ref(),
        transaction_directory.as_ref(),
        expected_binding,
        Some(expected_identity),
        completion,
    )
}

fn recover_transaction_inner(
    workspace_root: &Path,
    original_transaction_directory: &Path,
    expected_binding: RecoveryBinding,
    expected_identity: Option<crate::PatchIdentity>,
    completion: impl FnOnce(Option<&super::AppliedPatch>) -> Result<(), PatchError>,
) -> Result<RecoveryOutcome, PatchError> {
    let (workspace, transaction_directory) =
        recovery_roots(workspace_root, original_transaction_directory)?;
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
    if expected_identity.is_some_and(|expected| expected != manifest.identity) {
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

    classify_transaction(&workspace, &transaction_directory, manifest, completion)
}

fn classify_transaction(
    workspace: &Path,
    transaction_directory: &Path,
    mut manifest: Manifest,
    completion: impl FnOnce(Option<&super::AppliedPatch>) -> Result<(), PatchError>,
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
            None,
            completion,
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
                None,
                completion,
            )
        }
        TransactionPhase::Installing if facts.all_post => {
            manifest.phase = TransactionPhase::Installed;
            persist_manifest(transaction_directory, &manifest.encode()?)?;
            let installed = manifest.encode()?;
            completed_outcome(
                RecoveryState::AlreadyApplied,
                binding,
                manifest.identity,
                transaction_directory,
                Some(&installed),
                completion,
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
                None,
                completion,
            )
        }
        TransactionPhase::Installed if facts.all_post => completed_outcome(
            RecoveryState::AlreadyApplied,
            binding,
            manifest.identity,
            transaction_directory,
            Some(&manifest.encode()?),
            completion,
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
    installed_manifest: Option<&[u8]>,
    completion: impl FnOnce(Option<&super::AppliedPatch>) -> Result<(), PatchError>,
) -> Result<RecoveryOutcome, PatchError> {
    let parent = transaction_directory
        .parent()
        .ok_or_else(|| PatchError::indeterminate(PatchOperationContext::Cleanup))?;
    let applied =
        installed_manifest.map(|bytes| super::AppliedPatch::new(identity, bytes.to_vec(), false));
    if let Err(error) = completion(applied.as_ref()) {
        return Err(error.with_rollback(RollbackStatus::Indeterminate));
    }
    let cleanup_pending = cleanup_transaction(transaction_directory, parent).is_err();
    Ok(RecoveryOutcome::new(state, Some(binding), Some(identity), false, cleanup_pending))
}

fn rollback_io(error: io::Error) -> PatchError {
    PatchError::io(PatchOperationContext::Rollback, RollbackStatus::Indeterminate, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileMode, PatchIdentity, WorkspacePath};
    use peritus_types::{Generation, RevisionNumber, Sha256Digest, WorkspaceId};

    #[test]
    fn recovery_reads_complete_valid_manifest_above_former_payload_limit() {
        let directory = tempfile::tempdir().expect("transaction directory");
        let prefix = format!("{}/", "d".repeat(250)).repeat(16);
        let manifest = Manifest {
            phase: TransactionPhase::Prepared,
            workspace_id: WorkspaceId::new([1; 16]).expect("workspace"),
            generation: Generation::new(1).expect("generation"),
            revision: RevisionNumber::new(1).expect("revision"),
            identity: PatchIdentity::new(Sha256Digest::new([2; 32])),
            entries: (0..4_200)
                .map(|index| ManifestEntry {
                    kind: PatchOperationKind::Delete,
                    path: WorkspacePath::new(format!("{prefix}file-{index:05}"))
                        .expect("canonical path"),
                    preimage: Some(super::super::manifest::FileIdentity {
                        digest: Sha256Digest::new([3; 32]),
                        size: 1,
                        mode: FileMode::Regular,
                    }),
                    postimage: None,
                })
                .collect(),
            created_directories: Vec::new(),
        };
        let bytes = manifest.encode().expect("encode full manifest");
        assert!(bytes.len() > peritus_codec::CodecLimits::PRODUCTION.max_payload_bytes);
        fs::write(directory.path().join(MANIFEST_FILE), &bytes).expect("persist manifest");
        let retained = read_manifest(directory.path()).expect("read full recovery manifest");
        let decoded = Manifest::decode(&retained).expect("decode full recovery manifest");
        assert_eq!(retained, bytes);
        assert_eq!(decoded.entries, manifest.entries);
        assert_eq!(decoded.binding(), manifest.binding());
    }
}
