//! Read-only classification of restart-visible target and backup states.

use std::path::Path;

use crate::{ErrorCode, PatchError, PatchOperationContext, PatchOperationKind, RollbackStatus};

use super::{
    filesystem::{Observation, observation_matches, observe_absolute, observe_target},
    manifest::Manifest,
    storage::backup_path,
};

pub(super) struct RecoveryFacts {
    pub(super) all_pre: bool,
    pub(super) all_post: bool,
    pub(super) all_recoverable: bool,
}

/// Returns `None` when an unsafe target or backup prevents reliable observation.
pub(super) fn observe_manifest(
    workspace: &Path,
    transaction_directory: &Path,
    manifest: &Manifest,
) -> Result<Option<RecoveryFacts>, PatchError> {
    let mut all_pre = true;
    let mut all_post = true;
    let mut all_recoverable = true;
    for (index, entry) in manifest.entries.iter().enumerate() {
        let observed = match observe_target(
            workspace,
            &entry.path,
            PatchOperationContext::Recover,
            RollbackStatus::Indeterminate,
        ) {
            Ok(observed) => observed,
            Err(error) if error.code() == ErrorCode::UnsafeFilesystemTarget => return Ok(None),
            Err(error) => return Err(error),
        };
        let matches_pre = observation_matches(observed, entry.preimage);
        let matches_post = observation_matches(observed, entry.postimage);
        let backup_restores_replace =
            if observed == Observation::Absent && entry.kind == PatchOperationKind::Replace {
                let backup = backup_path(transaction_directory, index);
                match observe_absolute(
                    &backup,
                    PatchOperationContext::Recover,
                    RollbackStatus::Indeterminate,
                ) {
                    Ok(backup) => observation_matches(backup, entry.preimage),
                    Err(error) if error.code() == ErrorCode::UnsafeFilesystemTarget => {
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                }
            } else {
                false
            };
        all_pre &= matches_pre;
        all_post &= matches_post;
        all_recoverable &= crate::verified::target_is_recoverable(matches_pre, matches_post)
            || backup_restores_replace;
    }
    Ok(Some(RecoveryFacts { all_pre, all_post, all_recoverable }))
}
