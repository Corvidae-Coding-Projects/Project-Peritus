//! Authorized history-preserving rollback.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, FinalizedArtifact};
use peritus_git::{CandidateRequest, CandidateSnapshot, RestoreRequest, SnapshotRequest};
use peritus_types::{ActionId, SnapshotId};

use crate::{
    ErrorCode, RecoveryClass, RepositoryMutationKind, RepositoryMutationOperationReference,
    RepositoryMutationOutcomeReference, SnapshotIdentity, WorkspaceAuthorizationRequest,
    WorkspaceCondition, WorkspaceError, WorkspaceGateway, WorkspaceManifest, WorkspaceOperation,
};
use crate::{
    SnapshotPublicationFailure, finalize_snapshot_manifest,
    mutation_record::{persist_repository_operation, persist_repository_outcome},
};

const ROLLBACK_RESULT_MAGIC: &[u8] = b"PERITUS-WORKSPACE-ROLLBACK-RESULT-V1\0";

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
    receipt: RepositoryMutationOutcomeReference,
}

impl RollbackOutcome {
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
    /// Borrows the exact restart-visible action result.
    #[must_use]
    pub const fn receipt(&self) -> &RepositoryMutationOutcomeReference {
        &self.receipt
    }
}

/// Authorized durable rollback operation which has not crossed the Git effect boundary.
pub struct PreparedRollbackMutation {
    operation: RepositoryMutationOperationReference,
    target: CandidateSnapshot,
    successor_snapshot_id: SnapshotId,
}

impl PreparedRollbackMutation {
    /// Borrows the exact durable operation identity retained before mutation begins.
    #[must_use]
    pub const fn operation_reference(&self) -> &RepositoryMutationOperationReference {
        &self.operation
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
        let prepared = self.prepare_rollback(authorization, request)?;
        self.apply_prepared_rollback(prepared, artifacts)
    }

    /// Consumes rollback authority and retains its exact operation before any Git effect.
    ///
    /// # Errors
    /// Rejects lineage, counter, authority, or durable operation publication failures.
    pub fn prepare_rollback(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        request: RollbackRequest<'_>,
    ) -> Result<PreparedRollbackMutation, WorkspaceError> {
        self.state().current_snapshot().revision().checked_next().map_err(|_| {
            rollback_error(
                ErrorCode::RevisionExhausted,
                RecoveryClass::Quarantine,
                "workspace revision is exhausted",
            )
        })?;
        if request.target().workspace_id() != self.state().binding().workspace_id() {
            return Err(rollback_error(
                ErrorCode::ResourceMismatch,
                RecoveryClass::CorrectRequest,
                "rollback target belongs to another workspace lineage",
            ));
        }
        let payload = rollback_payload(self.state(), &request, authorization.caller_binding());
        let permit = self.authorize(authorization, &payload)?;
        let operation = RepositoryMutationOperationReference::new(
            RepositoryMutationKind::Rollback,
            &permit,
            self.state().binding().workspace_id(),
            self.state().binding().resource_id(),
            payload,
            authorization.caller_binding(),
        );
        persist_repository_operation(self.transaction_namespace(), &operation)?;
        Ok(PreparedRollbackMutation {
            operation,
            target: request.target().clone(),
            successor_snapshot_id: request.successor_snapshot_id(),
        })
    }

    /// Applies one already-authorized rollback, installs the new logical state, and durably
    /// settles its exact result before reporting success.
    ///
    /// # Errors
    /// Preserves typed Git, artifact, durable receipt, and workspace recovery failures.
    #[allow(
        clippy::too_many_lines,
        reason = "rollback keeps the ordered restore, retain, receipt, and state commit visible"
    )]
    pub fn apply_prepared_rollback(
        &mut self,
        prepared: PreparedRollbackMutation,
        artifacts: &ArtifactStore,
    ) -> Result<RollbackOutcome, WorkspaceError> {
        if prepared.operation.kind() != RepositoryMutationKind::Rollback
            || prepared.operation.workspace_id() != self.state().binding().workspace_id()
            || prepared.operation.resource_id() != self.state().binding().resource_id()
            || prepared.operation.generation() != self.state().generation()
            || prepared.operation.revision() != self.state().revision()
            || self.state().condition() != WorkspaceCondition::Clean
        {
            return Err(rollback_error(
                ErrorCode::StaleWorkspace,
                RecoveryClass::Reconcile,
                "prepared rollback differs from current workspace ownership",
            ));
        }
        let operation = prepared.operation;
        let target = prepared.target;
        let successor_snapshot_id = prepared.successor_snapshot_id;
        let prior = self.state().current_snapshot().clone();
        let next_revision = prior.revision().checked_next().map_err(|_| {
            rollback_error(
                ErrorCode::RevisionExhausted,
                RecoveryClass::Quarantine,
                "workspace revision is exhausted",
            )
        })?;
        let baseline_commit = self.state().binding().baseline_commit();
        let repository = self.workspace_mut().repository().clone();
        let worktree = self.workspace_mut().worktree().clone();
        let restored = repository
            .restore_snapshot(RestoreRequest::new(&worktree, &target, baseline_commit))
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
        if candidate.tree() != target.tree()
            || restored.restored_tree() != target.tree()
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
                successor_snapshot_id,
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
            operation.action_id(),
            operation.action_digest(),
            snapshot.tree(),
            candidate.manifest_digest(),
        );
        let artifact = finalize_snapshot_manifest(
            &repository,
            &snapshot,
            &manifest,
            artifacts,
            operation.dispatch_event(),
        )
        .map_err(|failure| rollback_publication_error(self, &failure))?;
        let receipt = RepositoryMutationOutcomeReference::new(
            operation,
            rollback_result_bytes(&target, &snapshot, &manifest, artifact.digest()),
        )?;
        self.workspace_mut().state_mut().install(identity.clone());
        if let Err(error) = persist_repository_outcome(self.transaction_namespace(), &receipt) {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
            return Err(error);
        }
        Ok(RollbackOutcome {
            action_id: receipt.operation().action_id(),
            restored_from: target.commit(),
            snapshot,
            identity,
            manifest,
            artifact,
            receipt,
        })
    }
}

const fn rollback_publication_error(
    gateway: &mut WorkspaceGateway,
    failure: &SnapshotPublicationFailure,
) -> WorkspaceError {
    let compensated = failure.compensation_failure().is_none();
    gateway.workspace_mut().state_mut().set_condition(if compensated {
        WorkspaceCondition::Dirty
    } else {
        WorkspaceCondition::Indeterminate
    });
    rollback_error(
        if compensated { ErrorCode::Artifact } else { ErrorCode::Git },
        RecoveryClass::Reconcile,
        if compensated {
            "rollback manifest was not finalized; its retained snapshot was released"
        } else {
            "rollback manifest failed and retained snapshot cleanup was inconclusive"
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

fn rollback_result_bytes(
    target: &CandidateSnapshot,
    snapshot: &CandidateSnapshot,
    manifest: &WorkspaceManifest,
    artifact: ArtifactDigest,
) -> Vec<u8> {
    let mut bytes = ROLLBACK_RESULT_MAGIC.to_vec();
    put_result_bytes(&mut bytes, target.manifest().bytes());
    put_result_bytes(&mut bytes, snapshot.manifest().bytes());
    put_result_bytes(&mut bytes, manifest.canonical_bytes());
    bytes.extend_from_slice(artifact.as_bytes());
    bytes
}

fn put_result_bytes(target: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("bounded rollback result length fits u64");
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
}

const fn rollback_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> WorkspaceError {
    WorkspaceError::new(code, WorkspaceOperation::Rollback, recovery, detail)
}
