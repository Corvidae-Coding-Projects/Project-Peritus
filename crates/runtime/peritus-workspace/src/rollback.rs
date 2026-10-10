//! Authorized history-preserving rollback.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, FinalizedArtifact};
use peritus_git::{CandidateRequest, CandidateSnapshot, RestoreRequest, SnapshotRequest};
use peritus_types::{ActionId, SnapshotId};

use crate::consumption::{self, ActionConsumptionBinding, ActionTerminalRecord};
use crate::{
    ErrorCode, RecoveryClass, SnapshotIdentity, WorkspaceAuthorizationRequest, WorkspaceCondition,
    WorkspaceError, WorkspaceGateway, WorkspaceManifest, WorkspaceOperation,
};
use crate::{SnapshotPublicationFailure, finalize_snapshot_manifest_recoverable};

/// Exact retained snapshot to restore and identity for its new successor snapshot.
#[derive(Clone, Copy, Debug)]
pub struct RollbackRequest<'a> {
    target: &'a CandidateSnapshot,
    successor_snapshot_id: SnapshotId,
}

impl<'a> RollbackRequest<'a> {
    /// Creates an unprivileged rollback request checked by the workspace gateway.
    #[must_use]
    pub const fn new(target: &'a CandidateSnapshot, successor_snapshot_id: SnapshotId) -> Self {
        Self { target, successor_snapshot_id }
    }

    /// Borrows the immutable retained target.
    #[must_use]
    pub const fn target(&self) -> &CandidateSnapshot {
        self.target
    }
    /// Returns the identity assigned to the new successor.
    #[must_use]
    pub const fn successor_snapshot_id(&self) -> SnapshotId {
        self.successor_snapshot_id
    }
}

/// New successor snapshot restoring old content without deleting history.
pub struct RollbackOutcome {
    action_id: ActionId,
    restored_from: peritus_git::CommitId,
    snapshot: CandidateSnapshot,
    identity: SnapshotIdentity,
    manifest: WorkspaceManifest,
    artifact: FinalizedArtifact,
}

impl RollbackOutcome {
    pub(crate) const fn from_receipt(
        action_id: ActionId,
        restored_from: peritus_git::CommitId,
        snapshot: CandidateSnapshot,
        identity: SnapshotIdentity,
        manifest: WorkspaceManifest,
        artifact: FinalizedArtifact,
    ) -> Self {
        Self { action_id, restored_from, snapshot, identity, manifest, artifact }
    }
    /// Returns the exact authorized action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the immutable commit whose tree was restored.
    #[must_use]
    pub const fn restored_from(&self) -> peritus_git::CommitId {
        self.restored_from
    }
    /// Borrows the newly retained successor snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &CandidateSnapshot {
        &self.snapshot
    }
    /// Returns the new logical snapshot identity.
    #[must_use]
    pub const fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }
    /// Returns the finalized rollback manifest artifact.
    #[must_use]
    pub const fn artifact_digest(&self) -> ArtifactDigest {
        self.artifact.digest()
    }
    /// Borrows exact canonical rollback evidence.
    #[must_use]
    pub const fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }
}

impl WorkspaceGateway {
    /// Restores a retained lineage snapshot as a new successor revision.
    ///
    /// # Errors
    ///
    /// Rejects another lineage before effect. Git or artifact failures leave the workspace
    /// indeterminate/dirty and preserve both old and new Git objects for reconciliation.
    #[allow(
        clippy::too_many_lines,
        reason = "rollback keeps the ordered restore, retain, artifact, and state commit visible"
    )]
    pub fn rollback(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        request: RollbackRequest<'_>,
        artifacts: &ArtifactStore,
    ) -> Result<RollbackOutcome, WorkspaceError> {
        let action_binding = ActionConsumptionBinding::from_state(self.state());
        let payload = rollback_payload(self.state(), &request, authorization.caller_binding());
        let permit = self.authorize(authorization, &payload)?;
        let prior = self.state().current_snapshot().clone();
        let next_revision = prior.revision().checked_next().map_err(|_| {
            rollback_error(
                ErrorCode::RevisionExhausted,
                RecoveryClass::Quarantine,
                "workspace revision is exhausted",
            )
        })?;
        self.workspace_mut().prepare_action_consumption(
            permit.action_id(),
            permit.action_digest(),
            &consumption::ActionPlan {
                operation: 4,
                snapshot_id: request.successor_snapshot_id(),
                payload_digest: peritus_codec::sha256(&payload),
                installed_revision: next_revision,
                dispatch_event: permit.dispatch_event(),
                patch_identity: None,
                patch_manifest_digest: None,
                target_snapshot_id: Some(request.target().snapshot_id()),
            },
        )?;
        let baseline_commit = self.state().binding().baseline_commit();
        if request.target().workspace_id() != self.state().binding().workspace_id() {
            return Err(rollback_error(
                ErrorCode::ResourceMismatch,
                RecoveryClass::CorrectRequest,
                "rollback target belongs to another workspace lineage",
            ));
        }
        let repository = self.workspace_mut().repository().clone();
        let worktree = self.workspace_mut().worktree().clone();
        let restored = repository
            .restore_snapshot(RestoreRequest::new(&worktree, request.target(), baseline_commit))
            .map_err(|_| {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                rollback_error(
                    ErrorCode::Git,
                    RecoveryClass::Reconcile,
                    "snapshot restore could not establish a complete result",
                )
            })?;
        self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Dirty);
        let candidate = repository
            .create_candidate(CandidateRequest::new(&worktree, baseline_commit))
            .map_err(|_| {
                rollback_error(
                    ErrorCode::Git,
                    RecoveryClass::Reconcile,
                    "restored result could not be written as an exact tree",
                )
            })?;
        if candidate.tree() != request.target().tree()
            || restored.restored_tree() != request.target().tree()
        {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Dirty);
            return Err(rollback_error(
                ErrorCode::Dirty,
                RecoveryClass::Reconcile,
                "restored content differs from the selected snapshot tree",
            ));
        }
        let snapshot = repository
            .create_snapshot(SnapshotRequest::new(
                &worktree,
                &candidate,
                prior.workspace_id(),
                request.successor_snapshot_id(),
                prior.commit(),
            ))
            .map_err(|_| {
                rollback_error(
                    ErrorCode::Git,
                    RecoveryClass::Reconcile,
                    "restored tree could not be retained as a successor snapshot",
                )
            })?;
        let identity = SnapshotIdentity::new(
            prior.workspace_id(),
            prior.generation(),
            next_revision,
            snapshot.commit(),
            snapshot.tree(),
        );
        let manifest = WorkspaceManifest::rollback(
            prior.workspace_id(),
            prior.generation(),
            prior.revision(),
            next_revision,
            permit.action_id(),
            permit.action_digest(),
            snapshot.tree(),
            candidate.manifest_digest(),
        );
        let artifact = finalize_snapshot_manifest_recoverable(
            &repository,
            &snapshot,
            &manifest,
            artifacts,
            permit.dispatch_event(),
        )
        .map_err(|failure| rollback_publication_error(self, &failure))?;
        let receipt = ActionTerminalRecord::WorkspaceRollback {
            restored_from: request.target().commit(),
            detail_digest: candidate.manifest_digest(),
            artifact_digest: artifact.digest(),
            artifact_size: artifact.size(),
            snapshot_manifest: snapshot.manifest().bytes().to_vec(),
            workspace_manifest: manifest.canonical_bytes().to_vec(),
        };
        if let Err(error) = consumption::complete_action(
            self.workspace_mut().transaction_root(),
            action_binding,
            permit.action_id(),
            permit.action_digest(),
            &receipt,
        ) {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
            return Err(error);
        }
        self.workspace_mut().state_mut().install(identity.clone());
        Ok(RollbackOutcome {
            action_id: permit.action_id(),
            restored_from: request.target().commit(),
            snapshot,
            identity,
            manifest,
            artifact,
        })
    }
}

const fn rollback_publication_error(
    gateway: &mut WorkspaceGateway,
    failure: &SnapshotPublicationFailure,
) -> WorkspaceError {
    let retained = failure.snapshot_retained();
    gateway.workspace_mut().state_mut().set_condition(if retained {
        WorkspaceCondition::Dirty
    } else {
        WorkspaceCondition::Indeterminate
    });
    rollback_error(
        if retained { ErrorCode::Artifact } else { ErrorCode::Git },
        RecoveryClass::Reconcile,
        if retained {
            "rollback manifest was not finalized; its retained snapshot is available for action recovery"
        } else {
            "rollback manifest failed and exact snapshot release was inconclusive"
        },
    )
}

/// Returns canonical payload bytes for an exact rollback action intent.
#[must_use]
pub fn rollback_authorization_payload(
    state: &crate::WorkspaceState,
    request: &RollbackRequest<'_>,
) -> Vec<u8> {
    rollback_payload(state, request, None)
}

/// Returns canonical rollback payload bytes bound to one exact validated C4 caller.
#[must_use]
pub fn rollback_authorization_payload_for_caller(
    state: &crate::WorkspaceState,
    request: &RollbackRequest<'_>,
    caller: &crate::WorkspaceCallerBinding,
) -> Vec<u8> {
    rollback_payload(state, request, Some(caller))
}

fn rollback_payload(
    state: &crate::WorkspaceState,
    request: &RollbackRequest<'_>,
    caller: Option<&crate::WorkspaceCallerBinding>,
) -> Vec<u8> {
    let mut bytes = if caller.is_some() {
        b"PERITUS-WORKSPACE-ROLLBACK-V2\0".to_vec()
    } else {
        b"PERITUS-WORKSPACE-ROLLBACK-V1\0".to_vec()
    };
    bytes.extend_from_slice(state.binding().workspace_id().as_bytes());
    bytes.extend_from_slice(&state.generation().get().to_be_bytes());
    bytes.extend_from_slice(&state.revision().get().to_be_bytes());
    put_object(&mut bytes, request.target().commit().object_id());
    put_object(&mut bytes, request.target().tree().object_id());
    bytes.extend_from_slice(request.successor_snapshot_id().as_bytes());
    if let Some(caller) = caller {
        crate::caller::append_caller(&mut bytes, Some(caller));
    }
    bytes
}

fn put_object(bytes: &mut Vec<u8>, object: peritus_git::ObjectId) {
    bytes.push(match object.format() {
        peritus_git::ObjectFormat::Sha1 => 1,
        peritus_git::ObjectFormat::Sha256 => 2,
    });
    bytes.extend_from_slice(object.as_bytes());
}

const fn rollback_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> WorkspaceError {
    WorkspaceError::new(code, WorkspaceOperation::Rollback, recovery, detail)
}
