//! Finish a retained, preplanned Git effect before publishing its exact receipt.

use super::{
    ActionConsumptionBinding, ActionId, ActionTerminalRecord, ArtifactStore, CandidateOutcome,
    CandidateRequest, GitMutationRecoveryOutcome, RollbackOutcome, Sha256Digest, SnapshotIdentity,
    SnapshotRequest, WorkspaceError, WorkspaceGateway, WorkspaceManifest, consumption,
    recovery_error,
};

impl WorkspaceGateway {
    #[allow(
        clippy::too_many_lines,
        reason = "planned recovery proves the exact installed Git snapshot before finalizing its receipt"
    )]
    pub(super) fn recover_planned_git_mutation(
        &mut self,
        binding: ActionConsumptionBinding,
        action_id: ActionId,
        action_digest: Sha256Digest,
        plan: &consumption::ActionPlan,
        artifacts: &ArtifactStore,
    ) -> Result<GitMutationRecoveryOutcome, WorkspaceError> {
        let successor = binding
            .revision()
            .checked_next()
            .map_err(|_| recovery_error("planned mutation revision cannot advance"))?;
        let current_revision = self.state().revision();
        if plan.installed_revision != successor
            || (current_revision != binding.revision() && current_revision != successor)
        {
            return Err(recovery_error(
                "planned mutation is outside the exact prior/successor revision",
            ));
        }
        let workspace_id = binding.workspace_id();
        let repository = self.writable_workspace().repository().clone();
        let worktree = self.writable_workspace().worktree().clone();
        let snapshot = match repository
            .recover_snapshot_id(workspace_id, plan.snapshot_id)
            .map_err(|_| recovery_error("planned Git snapshot evidence is malformed or drifted"))?
        {
            Some(snapshot) => snapshot,
            None if plan.operation == 4 => {
                let Some(target_id) = plan.target_snapshot_id else {
                    return Err(recovery_error("rollback plan lacks its target snapshot"));
                };
                let target = repository
                    .reopen_snapshot_id(workspace_id, target_id)
                    .map_err(|_| {
                        recovery_error("rollback target evidence is malformed or drifted")
                    })?
                    .ok_or_else(|| recovery_error("rollback target evidence is missing"))?;
                let baseline = self.state().binding().baseline_commit();
                let observed = repository.status(&worktree).map_err(|_| {
                    recovery_error("interrupted rollback worktree cannot be verified")
                })?;
                if observed.head() != Some(baseline)
                    || observed.index_tree() != Some(target.tree())
                    || !observed.worktree_matches_index()
                {
                    return Ok(GitMutationRecoveryOutcome::Incomplete);
                }
                let candidate = repository
                    .create_candidate(CandidateRequest::new(&worktree, baseline))
                    .map_err(|_| {
                        recovery_error(
                            "verified rollback result could not be finalized as a candidate",
                        )
                    })?;
                if candidate.tree() != target.tree() {
                    return Err(recovery_error(
                        "verified rollback candidate differs from its target",
                    ));
                }
                repository
                    .create_snapshot(SnapshotRequest::new(
                        &worktree,
                        &candidate,
                        workspace_id,
                        plan.snapshot_id,
                        self.state().current_snapshot().commit(),
                    ))
                    .map_err(|_| {
                        recovery_error("verified rollback successor could not be retained")
                    })?
            }
            None => return Ok(GitMutationRecoveryOutcome::Incomplete),
        };
        if current_revision == binding.revision()
            && snapshot.manifest().parent() != self.state().current_snapshot().commit()
        {
            return Err(recovery_error(
                "planned snapshot parent differs from the current workspace snapshot",
            ));
        }
        let status = repository
            .status(&worktree)
            .map_err(|_| recovery_error("planned worktree state cannot be verified"))?;
        if status.head() != Some(self.state().binding().baseline_commit())
            || status.index_tree() != Some(snapshot.tree())
            || !status.worktree_matches_index()
        {
            return Err(recovery_error(
                "planned snapshot is not the exact installed worktree and index state",
            ));
        }
        let identity = SnapshotIdentity::new(
            workspace_id,
            binding.generation(),
            successor,
            snapshot.commit(),
            snapshot.tree(),
        );
        if current_revision == successor && self.state().current_snapshot() != &identity {
            return Err(recovery_error("planned snapshot differs from the installed successor"));
        }
        let (terminal, detail_digest) = match plan.operation {
            3 => {
                let patch_identity = plan
                    .patch_identity
                    .ok_or_else(|| recovery_error("candidate plan lacks its patch identity"))?;
                let patch_digest = plan
                    .patch_manifest_digest
                    .ok_or_else(|| recovery_error("candidate plan lacks its patch digest"))?;
                let detail = crate::candidate::combined_detail(
                    patch_digest,
                    snapshot.manifest().candidate_digest(),
                );
                let manifest = WorkspaceManifest::candidate(
                    workspace_id,
                    binding.generation(),
                    binding.revision(),
                    successor,
                    action_id,
                    action_digest,
                    snapshot.tree(),
                    detail,
                );
                let artifact = manifest.finalize(artifacts, plan.dispatch_event).map_err(|_| {
                    recovery_error("candidate recovery artifact cannot be finalized")
                })?;
                (
                    ActionTerminalRecord::Candidate {
                        patch_identity,
                        detail_digest: detail,
                        artifact_digest: artifact.digest(),
                        artifact_size: artifact.size(),
                        snapshot_manifest: snapshot.manifest().bytes().to_vec(),
                        workspace_manifest: manifest.canonical_bytes().to_vec(),
                    },
                    detail,
                )
            }
            4 => {
                let target_id = plan
                    .target_snapshot_id
                    .ok_or_else(|| recovery_error("rollback plan lacks its target snapshot"))?;
                let target = repository
                    .reopen_snapshot_id(workspace_id, target_id)
                    .map_err(|_| {
                        recovery_error("rollback target evidence is malformed or drifted")
                    })?
                    .ok_or_else(|| recovery_error("rollback target evidence is missing"))?;
                if target.tree() != snapshot.tree() {
                    return Err(recovery_error(
                        "rollback successor differs from its planned target",
                    ));
                }
                let detail = snapshot.manifest().candidate_digest();
                let manifest = WorkspaceManifest::rollback(
                    workspace_id,
                    binding.generation(),
                    binding.revision(),
                    successor,
                    action_id,
                    action_digest,
                    snapshot.tree(),
                    detail,
                );
                let artifact = manifest.finalize(artifacts, plan.dispatch_event).map_err(|_| {
                    recovery_error("rollback recovery artifact cannot be finalized")
                })?;
                (
                    ActionTerminalRecord::WorkspaceRollback {
                        restored_from: target.commit(),
                        detail_digest: detail,
                        artifact_digest: artifact.digest(),
                        artifact_size: artifact.size(),
                        snapshot_manifest: snapshot.manifest().bytes().to_vec(),
                        workspace_manifest: manifest.canonical_bytes().to_vec(),
                    },
                    detail,
                )
            }
            _ => return Err(recovery_error("planned Git operation is unsupported")),
        };
        consumption::complete_action(
            self.writable_workspace().transaction_root(),
            binding,
            action_id,
            action_digest,
            &terminal,
        )?;
        if current_revision == binding.revision() {
            self.workspace_mut().state_mut().install(identity.clone());
        }
        let artifact_digest = match &terminal {
            ActionTerminalRecord::Candidate { artifact_digest, .. }
            | ActionTerminalRecord::WorkspaceRollback { artifact_digest, .. } => *artifact_digest,
            _ => unreachable!("planned operation produces a typed Git receipt"),
        };
        let artifact = artifacts
            .reopen_finalized(artifact_digest)
            .map_err(|_| recovery_error("planned mutation artifact cannot be reopened"))?;
        match terminal {
            ActionTerminalRecord::Candidate { patch_identity, .. } => {
                Ok(GitMutationRecoveryOutcome::Candidate(CandidateOutcome::from_receipt(
                    action_id,
                    patch_identity,
                    snapshot,
                    identity.clone(),
                    WorkspaceManifest::candidate(
                        workspace_id,
                        binding.generation(),
                        binding.revision(),
                        successor,
                        action_id,
                        action_digest,
                        identity.tree(),
                        detail_digest,
                    ),
                    artifact,
                )))
            }
            ActionTerminalRecord::WorkspaceRollback { restored_from, .. } => {
                Ok(GitMutationRecoveryOutcome::Rollback(RollbackOutcome::from_receipt(
                    action_id,
                    restored_from,
                    snapshot,
                    identity.clone(),
                    WorkspaceManifest::rollback(
                        workspace_id,
                        binding.generation(),
                        binding.revision(),
                        successor,
                        action_id,
                        action_digest,
                        identity.tree(),
                        detail_digest,
                    ),
                    artifact,
                )))
            }
            _ => unreachable!("planned operation produces a typed Git receipt"),
        }
    }
}
