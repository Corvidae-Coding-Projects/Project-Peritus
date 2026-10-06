//! Prepared rewind reconciliation against exact C1 and retained checkpoint evidence.

use peritus_app_protocol::{
    WorkbenchCommand, WorkbenchIntent, WorkbenchRewindDisposition, WorkbenchRewindMode,
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
        evidence: (&UserCheckpoint, &UserCheckpoint),
    ) -> Result<RecoveredRestore, Error> {
        let (checkpoint, recovery) = evidence;
        let WorkbenchIntent::ApplyRewind(confirmed) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if checkpoint.id() != restore.checkpoint() {
            return Err(Error::Corrupt("restore source checkpoint identity differs"));
        }
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
                    WorkbenchRewindDisposition::Conflict
                        | WorkbenchRewindDisposition::Unsealed
                        | WorkbenchRewindDisposition::Unavailable
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
            match self.restore_plan_materialized(
                store,
                command.query(),
                checkpoint,
                confirmed,
                Some(restore.patch_digest()),
                Some((recovery, None)),
            ) {
                Ok(plan) => (true, plan),
                Err(_) => (false, None),
            }
        } else {
            (false, None)
        };
        let targets_exact = restore.targets().is_none_or(|targets| {
            let versions = super::rewind::planned_versions(recovery, plan.as_ref());
            versions.len() == targets.len()
                && targets.iter().zip(versions).all(|(target, (path, version))| {
                    target.path() == path && target.checkpoint() == version
                })
        });
        let plan_exact = if !structurally_exact || !targets_exact {
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
            .observe_checkpoint_paths(record, command.query(), recovery)
            .ok()
            .filter(|paths| paths.len() == confirmed.paths().len());
        let versions = observed.as_ref().map(|paths| {
            paths.iter().map(|path| path.version).collect::<Vec<CheckpointFileVersion>>()
        });

        let (all_pre, all_post) =
            classify_restore_paths(confirmed, recovery, versions.as_deref(), restore.targets());
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
                    && super::projection::public_ranges(target)
                        .is_ok_and(|ranges| ranges == shown.ranges())
            },
        )
}

fn classify_restore_paths(
    preview: &peritus_app_protocol::WorkbenchRewindPreview,
    recovery: &UserCheckpoint,
    observed: Option<&[CheckpointFileVersion]>,
    targets: Option<&[peritus_product_runner::control::CheckpointPath]>,
) -> (bool, bool) {
    let Some(observed) = observed else { return (false, false) };
    let mut all_pre = true;
    let mut all_post = true;
    for (index, ((shown, before), current)) in
        preview.paths().iter().zip(recovery.paths()).zip(observed).enumerate()
    {
        all_pre &= *current == before.checkpoint();
        all_post &= match targets {
            Some(targets) => targets.get(index).is_some_and(|target| {
                target.path() == shown.path() && target.checkpoint() == *current
            }),
            None => public_version(*current) == shown.checkpoint(),
        };
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

mod evidence;
use evidence::encode_evidence;
#[cfg(test)]
mod tests;
