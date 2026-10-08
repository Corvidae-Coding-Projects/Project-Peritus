//! Durable replay of completed candidate and rollback actions.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_git::{CandidateRequest, SnapshotRequest};
use peritus_types::{ActionId, Sha256Digest};

use crate::{
    CandidateOutcome, ErrorCode, RecoveryClass, RollbackOutcome, SnapshotIdentity, WorkspaceError,
    WorkspaceGateway, WorkspaceManifest, WorkspaceOperation,
    consumption::{self, ActionConsumptionBinding, ActionTerminalRecord},
};

mod planned;

/// Exact recovered Git mutation or a marker that has not yet reached a terminal outcome.
pub enum GitMutationRecoveryOutcome {
    /// No action marker exists for this exact prior workspace revision.
    NotFound,
    /// The action was consumed but a durable Git outcome has not yet been recorded.
    Incomplete,
    /// Candidate receipt reconstructed and verified without repeating Git effects.
    Candidate(CandidateOutcome),
    /// Rollback receipt reconstructed and verified without repeating Git effects.
    Rollback(RollbackOutcome),
}

impl WorkspaceGateway {
    /// Reopens the exact durable receipt for a completed candidate or rollback action.
    ///
    /// The prior revision binding selects the marker, while the current installed snapshot must
    /// match the exact successor recorded there. No Git mutation is retried.
    ///
    /// # Errors
    /// Rejects another lineage, digest mismatch, malformed receipt, missing artifact, or drift.
    pub fn recover_git_mutation(
        &mut self,
        binding: ActionConsumptionBinding,
        action_id: ActionId,
        action_digest: Sha256Digest,
        payload_digest: Sha256Digest,
        artifacts: &ArtifactStore,
    ) -> Result<GitMutationRecoveryOutcome, WorkspaceError> {
        let state = self.state();
        let workspace = state.binding();
        let successor_revision = binding
            .revision()
            .checked_next()
            .map_err(|_| recovery_error("Git mutation receipt revision cannot advance"))?;
        if binding.workspace_id() != workspace.workspace_id()
            || binding.resource_id() != workspace.resource_id()
            || binding.environment_id() != workspace.environment_id()
            || binding.generation() != state.generation()
            || (state.revision() != successor_revision && state.revision() != binding.revision())
        {
            return Err(recovery_error("Git mutation receipt is not the exact prior revision"));
        }
        let Some(record) = consumption::action_record(
            self.writable_workspace().transaction_root(),
            binding,
            action_id,
        )?
        else {
            return Ok(GitMutationRecoveryOutcome::NotFound);
        };
        if record.action_digest != action_digest {
            return Err(recovery_error("Git mutation receipt digest differs from the action"));
        }
        if record.plan.as_ref().is_some_and(|plan| plan.payload_digest != payload_digest) {
            return Err(recovery_error("Git mutation plan differs from the exact action payload"));
        }
        let Some(terminal) = record.terminal else {
            if let Some(plan) = record.plan.as_ref() {
                return self.recover_planned_git_mutation(
                    binding,
                    action_id,
                    action_digest,
                    plan,
                    artifacts,
                );
            }
            return Ok(GitMutationRecoveryOutcome::Incomplete);
        };
        if state.revision() != successor_revision && state.revision() != binding.revision() {
            return Err(recovery_error(
                "durable Git outcome is outside its prior/successor revision",
            ));
        }
        match terminal {
            ActionTerminalRecord::Candidate {
                patch_identity,
                detail_digest,
                artifact_digest,
                artifact_size,
                snapshot_manifest,
                workspace_manifest,
            } => {
                let outcome = self.reopen_candidate(
                    binding,
                    action_id,
                    action_digest,
                    patch_identity,
                    detail_digest,
                    artifact_digest,
                    artifact_size,
                    &snapshot_manifest,
                    &workspace_manifest,
                    artifacts,
                )?;
                if state.revision() == binding.revision() {
                    self.workspace_mut().state_mut().install(outcome.identity().clone());
                }
                Ok(GitMutationRecoveryOutcome::Candidate(outcome))
            }
            ActionTerminalRecord::WorkspaceRollback {
                restored_from,
                detail_digest,
                artifact_digest,
                artifact_size,
                snapshot_manifest,
                workspace_manifest,
            } => {
                let outcome = self.reopen_rollback(
                    binding,
                    action_id,
                    action_digest,
                    restored_from,
                    detail_digest,
                    artifact_digest,
                    artifact_size,
                    &snapshot_manifest,
                    &workspace_manifest,
                    artifacts,
                )?;
                if state.revision() == binding.revision() {
                    self.workspace_mut().state_mut().install(outcome.identity().clone());
                }
                Ok(GitMutationRecoveryOutcome::Rollback(outcome))
            }
            _ => Ok(GitMutationRecoveryOutcome::Incomplete),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "replay validates each persisted candidate identity"
    )]
    fn reopen_candidate(
        &self,
        binding: ActionConsumptionBinding,
        action_id: ActionId,
        action_digest: Sha256Digest,
        patch_identity: peritus_patch::PatchIdentity,
        detail_digest: Sha256Digest,
        artifact_digest: ArtifactDigest,
        artifact_size: u64,
        snapshot_manifest: &[u8],
        workspace_manifest: &[u8],
        artifacts: &ArtifactStore,
    ) -> Result<CandidateOutcome, WorkspaceError> {
        let manifest = peritus_git::CandidateSnapshotManifest::decode(snapshot_manifest)
            .map_err(|_| recovery_error("candidate snapshot receipt is malformed"))?;
        let snapshot = self
            .writable_workspace()
            .repository()
            .reopen_snapshot(&manifest)
            .map_err(|_| recovery_error("candidate snapshot receipt no longer resolves"))?;
        if snapshot.workspace_id() != binding.workspace_id() {
            return Err(recovery_error("candidate snapshot belongs to another workspace"));
        }
        let next_revision = binding
            .revision()
            .checked_next()
            .map_err(|_| recovery_error("candidate result revision cannot be reconstructed"))?;
        let identity = SnapshotIdentity::new(
            binding.workspace_id(),
            binding.generation(),
            next_revision,
            snapshot.commit(),
            snapshot.tree(),
        );
        if self.state().revision() == next_revision && &identity != self.state().current_snapshot()
        {
            return Err(recovery_error("candidate result differs from installed workspace state"));
        }
        self.verify_recovered_worktree(binding, snapshot.tree())?;
        if self.state().revision() == binding.revision()
            && snapshot.manifest().parent() != self.state().current_snapshot().commit()
        {
            return Err(recovery_error(
                "candidate result does not descend from the current workspace snapshot",
            ));
        }
        let manifest = WorkspaceManifest::candidate(
            binding.workspace_id(),
            binding.generation(),
            binding.revision(),
            next_revision,
            action_id,
            action_digest,
            snapshot.tree(),
            detail_digest,
        );
        Self::verify_receipt_artifact(
            &manifest,
            artifact_digest,
            artifact_size,
            workspace_manifest,
            artifacts,
        )?;
        Ok(CandidateOutcome::from_receipt(
            action_id,
            patch_identity,
            snapshot,
            identity,
            manifest,
            artifacts
                .reopen_finalized(artifact_digest)
                .map_err(|_| recovery_error("candidate artifact cannot be reopened"))?,
        ))
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "replay validates each persisted rollback identity"
    )]
    fn reopen_rollback(
        &self,
        binding: ActionConsumptionBinding,
        action_id: ActionId,
        action_digest: Sha256Digest,
        restored_from: peritus_git::CommitId,
        detail_digest: Sha256Digest,
        artifact_digest: ArtifactDigest,
        artifact_size: u64,
        snapshot_manifest: &[u8],
        workspace_manifest: &[u8],
        artifacts: &ArtifactStore,
    ) -> Result<RollbackOutcome, WorkspaceError> {
        let manifest = peritus_git::CandidateSnapshotManifest::decode(snapshot_manifest)
            .map_err(|_| recovery_error("rollback snapshot receipt is malformed"))?;
        let snapshot = self
            .writable_workspace()
            .repository()
            .reopen_snapshot(&manifest)
            .map_err(|_| recovery_error("rollback snapshot receipt no longer resolves"))?;
        if snapshot.workspace_id() != binding.workspace_id() {
            return Err(recovery_error("rollback snapshot belongs to another workspace"));
        }
        let next_revision = binding
            .revision()
            .checked_next()
            .map_err(|_| recovery_error("rollback result revision cannot be reconstructed"))?;
        let identity = SnapshotIdentity::new(
            binding.workspace_id(),
            binding.generation(),
            next_revision,
            snapshot.commit(),
            snapshot.tree(),
        );
        if self.state().revision() == next_revision && &identity != self.state().current_snapshot()
        {
            return Err(recovery_error("rollback result differs from installed workspace state"));
        }
        self.verify_recovered_worktree(binding, snapshot.tree())?;
        if self.state().revision() == binding.revision()
            && snapshot.manifest().parent() != self.state().current_snapshot().commit()
        {
            return Err(recovery_error(
                "rollback result does not descend from the current workspace snapshot",
            ));
        }
        let manifest = WorkspaceManifest::rollback(
            binding.workspace_id(),
            binding.generation(),
            binding.revision(),
            next_revision,
            action_id,
            action_digest,
            snapshot.tree(),
            detail_digest,
        );
        Self::verify_receipt_artifact(
            &manifest,
            artifact_digest,
            artifact_size,
            workspace_manifest,
            artifacts,
        )?;
        Ok(RollbackOutcome::from_receipt(
            action_id,
            restored_from,
            snapshot,
            identity,
            manifest,
            artifacts
                .reopen_finalized(artifact_digest)
                .map_err(|_| recovery_error("rollback artifact cannot be reopened"))?,
        ))
    }

    fn verify_receipt_artifact(
        manifest: &WorkspaceManifest,
        artifact_digest: ArtifactDigest,
        artifact_size: u64,
        expected_bytes: &[u8],
        artifacts: &ArtifactStore,
    ) -> Result<(), WorkspaceError> {
        if manifest.digest() != artifact_digest
            || manifest.canonical_bytes() != expected_bytes
            || u64::try_from(expected_bytes.len()).ok() != Some(artifact_size)
            || !matches!(artifacts.read(artifact_digest, artifact_size), Ok(actual) if actual == expected_bytes)
        {
            return Err(recovery_error("workspace manifest artifact differs from its receipt"));
        }
        Ok(())
    }

    fn verify_recovered_worktree(
        &self,
        binding: ActionConsumptionBinding,
        expected_tree: peritus_git::TreeId,
    ) -> Result<(), WorkspaceError> {
        let workspace = self.writable_workspace();
        let status = workspace
            .repository()
            .status(workspace.worktree())
            .map_err(|_| recovery_error("recovered Git mutation worktree cannot be verified"))?;
        if status.head() != Some(workspace.state().binding().baseline_commit())
            || status.index_tree() != Some(expected_tree)
            || !status.worktree_matches_index()
            || binding.workspace_id() != workspace.state().binding().workspace_id()
        {
            return Err(recovery_error(
                "recovered Git mutation no longer matches its exact installed workspace state",
            ));
        }
        Ok(())
    }
}

const fn recovery_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Candidate,
        RecoveryClass::Reconcile,
        detail,
    )
}
