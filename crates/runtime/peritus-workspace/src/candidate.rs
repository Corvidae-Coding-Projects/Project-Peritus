//! Authorized candidate creation and content-addressed manifest finalization.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, FinalizedArtifact};
use peritus_git::{CandidateRequest, CandidateSnapshot, SnapshotRequest};
use peritus_patch::PatchIdentity;
use peritus_types::{ActionId, Generation, ResourceId, RevisionNumber, SnapshotId, WorkspaceId};

use crate::{
    ErrorCode, MutationOutcome, MutationOutcomeReference, RecoveryClass, SnapshotIdentity,
    RepositoryMutationKind, RepositoryMutationOperationReference,
    RepositoryMutationOutcomeReference, WorkspaceAuthorizationRequest, WorkspaceCondition,
    WorkspaceError, WorkspaceGateway, WorkspaceManifest, WorkspaceOperation,
};
use crate::{
    SnapshotPublicationFailure, finalize_snapshot_manifest,
    mutation_record::{persist_repository_operation, persist_repository_outcome},
};

const CANDIDATE_RESULT_MAGIC: &[u8] = b"PERITUS-WORKSPACE-CANDIDATE-RESULT-V1\0";

/// Retained immutable candidate and its finalized C0 artifact observation.
pub struct CandidateOutcome {
    action_id: ActionId,
    patch_id: PatchIdentity,
    snapshot: CandidateSnapshot,
    identity: SnapshotIdentity,
    manifest: WorkspaceManifest,
    artifact: FinalizedArtifact,
    receipt: RepositoryMutationOutcomeReference,
}

impl CandidateOutcome {
    /// Returns the exact authorized action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the installed patch identity incorporated by the candidate.
    #[must_use]
    pub const fn patch_id(&self) -> PatchIdentity {
        self.patch_id
    }
    /// Borrows the retained Git snapshot registration.
    #[must_use]
    pub const fn snapshot(&self) -> &CandidateSnapshot {
        &self.snapshot
    }
    /// Returns the exact immutable workspace identity.
    #[must_use]
    pub const fn identity(&self) -> &SnapshotIdentity {
        &self.identity
    }
    /// Returns the finalized manifest artifact identity.
    #[must_use]
    pub const fn artifact_digest(&self) -> ArtifactDigest {
        self.artifact.digest()
    }
    /// Borrows the canonical outcome manifest.
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

/// Authorized durable candidate operation which has not crossed the Git effect boundary.
pub struct PreparedCandidateMutation {
    operation: RepositoryMutationOperationReference,
    mutation: MutationOutcomeReference,
    snapshot_id: SnapshotId,
}

impl PreparedCandidateMutation {
    /// Borrows the exact durable operation identity retained before mutation begins.
    #[must_use]
    pub const fn operation_reference(&self) -> &RepositoryMutationOperationReference {
        &self.operation
    }
}

impl WorkspaceGateway {
    /// Creates a candidate from the live outcome retained by a legacy in-process caller.
    ///
    /// # Errors
    /// Preserves candidate authorization, Git, artifact, and workspace failures.
    pub fn create_candidate(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        mutation: &MutationOutcome,
        snapshot_id: SnapshotId,
        artifacts: &ArtifactStore,
    ) -> Result<CandidateOutcome, WorkspaceError> {
        let prepared = self.prepare_candidate(
            authorization,
            mutation,
            snapshot_id,
        )?;
        self.create_prepared_candidate(prepared, artifacts)
    }

    /// Consumes candidate authority and retains its exact operation before any Git effect.
    ///
    /// # Errors
    /// Rejects stale patch evidence, authority mismatch, or durable operation publication failure.
    pub fn prepare_candidate(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        mutation: &MutationOutcome,
        snapshot_id: SnapshotId,
    ) -> Result<PreparedCandidateMutation, WorkspaceError> {
        let payload = candidate_payload(mutation, snapshot_id, authorization.caller_binding());
        self.prepare_candidate_reference_inner(
            authorization,
            mutation.reference(),
            snapshot_id,
            payload,
        )
    }

    /// Resolves a target-owned durable mutation outcome and creates its exact candidate.
    ///
    /// # Errors
    /// Rejects unavailable, forged, stale, foreign, or otherwise invalid outcome references.
    pub fn create_candidate_from_reference(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        mutation: MutationOutcomeReference,
        snapshot_id: SnapshotId,
        artifacts: &ArtifactStore,
    ) -> Result<CandidateOutcome, WorkspaceError> {
        let prepared = self.prepare_candidate_from_reference(
            authorization,
            mutation,
            snapshot_id,
        )?;
        self.create_prepared_candidate(prepared, artifacts)
    }

    /// Resolves a patch handoff, consumes candidate authority, and retains the operation before
    /// any Git effect.
    ///
    /// # Errors
    /// Rejects unavailable or stale patch evidence, authority mismatch, or record failure.
    pub fn prepare_candidate_from_reference(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        mutation: MutationOutcomeReference,
        snapshot_id: SnapshotId,
    ) -> Result<PreparedCandidateMutation, WorkspaceError> {
        let mutation = crate::mutation_record::resolve_outcome(
            self.transaction_namespace(),
            mutation,
        )?;
        let payload = candidate_payload_reference(
            mutation,
            snapshot_id,
            authorization.caller_binding(),
        );
        self.prepare_candidate_reference_inner(authorization, mutation, snapshot_id, payload)
    }

    /// Validates the exact applied patch, consumes candidate authority, and publishes the durable
    /// pre-effect operation record.
    ///
    /// # Errors
    ///
    /// Returns before Git effect on stale input, authority, or operation-record failure.
    fn prepare_candidate_reference_inner(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        mutation: MutationOutcomeReference,
        snapshot_id: SnapshotId,
        payload: Vec<u8>,
    ) -> Result<PreparedCandidateMutation, WorkspaceError> {
        validate_mutation_input(self.state(), mutation)?;
        let permit =
            self.authorize_in_condition(authorization, &payload, WorkspaceCondition::Dirty)?;
        validate_mutation_input(self.state(), mutation)?;
        if mutation.generation() != permit.generation() || mutation.revision() != permit.revision()
        {
            return Err(candidate_error(
                ErrorCode::StaleWorkspace,
                RecoveryClass::Reauthorize,
                "patch outcome differs from candidate permit",
            ));
        }
        let operation = RepositoryMutationOperationReference::new(
            RepositoryMutationKind::Candidate,
            &permit,
            self.state().binding().workspace_id(),
            self.state().binding().resource_id(),
            payload,
            authorization.caller_binding(),
        );
        persist_repository_operation(self.transaction_namespace(), &operation)?;
        Ok(PreparedCandidateMutation { operation, mutation, snapshot_id })
    }

    /// Applies one already-authorized candidate operation, installs the new logical state, and
    /// durably settles its exact result before reporting success.
    ///
    /// # Errors
    /// Preserves typed Git, artifact, durable receipt, and workspace recovery failures.
    pub fn create_prepared_candidate(
        &mut self,
        prepared: PreparedCandidateMutation,
        artifacts: &ArtifactStore,
    ) -> Result<CandidateOutcome, WorkspaceError> {
        if prepared.operation.kind() != RepositoryMutationKind::Candidate
            || prepared.operation.workspace_id() != self.state().binding().workspace_id()
            || prepared.operation.resource_id() != self.state().binding().resource_id()
            || prepared.operation.generation() != self.state().generation()
            || prepared.operation.revision() != self.state().revision()
            || self.state().condition() != WorkspaceCondition::Dirty
        {
            return Err(candidate_error(
                ErrorCode::StaleWorkspace,
                RecoveryClass::Reconcile,
                "prepared candidate differs from current workspace ownership",
            ));
        }
        validate_mutation_input(self.state(), prepared.mutation)?;
        let operation = prepared.operation;
        let mutation = prepared.mutation;
        let snapshot_id = prepared.snapshot_id;
        let prior = self.state().current_snapshot().clone();
        let repository = self.workspace_mut().repository().clone();
        let worktree = self.workspace_mut().worktree().clone();
        let candidate = repository
            .create_candidate(CandidateRequest::new(
                &worktree,
                self.state().binding().baseline_commit(),
            ))
            .map_err(|_| {
                candidate_error(
                    ErrorCode::Git,
                    RecoveryClass::Reconcile,
                    "Git could not create the exact candidate tree",
                )
            })?;
        let snapshot = repository
            .create_snapshot(SnapshotRequest::new(
                &worktree,
                &candidate,
                self.state().binding().workspace_id(),
                snapshot_id,
                prior.commit(),
            ))
            .map_err(|_| {
                candidate_error(
                    ErrorCode::Git,
                    RecoveryClass::Reconcile,
                    "Git could not retain the candidate snapshot",
                )
            })?;
        let next_revision = prior.revision().checked_next().map_err(|_| {
            candidate_error(
                ErrorCode::RevisionExhausted,
                RecoveryClass::Quarantine,
                "workspace revision is exhausted",
            )
        })?;
        let identity = SnapshotIdentity::new(
            prior.workspace_id(),
            prior.generation(),
            next_revision,
            snapshot.commit(),
            snapshot.tree(),
        );
        let detail_digest = combined_detail(
            mutation.installed_manifest_digest(),
            candidate.manifest_digest(),
        );
        let manifest = WorkspaceManifest::candidate(
            prior.workspace_id(),
            prior.generation(),
            prior.revision(),
            next_revision,
            operation.action_id(),
            operation.action_digest(),
            snapshot.tree(),
            detail_digest,
        );
        let artifact = finalize_snapshot_manifest(
            &repository,
            &snapshot,
            &manifest,
            artifacts,
            operation.dispatch_event(),
        )
        .map_err(|failure| candidate_publication_error(self, &failure))?;
        let receipt = RepositoryMutationOutcomeReference::new(
            operation,
            candidate_result_bytes(
                mutation.patch_identity(),
                &snapshot,
                &manifest,
                artifact.digest(),
            ),
        )?;
        self.workspace_mut().state_mut().install(identity.clone());
        if let Err(error) = persist_repository_outcome(self.transaction_namespace(), &receipt) {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
            return Err(error);
        }
        Ok(CandidateOutcome {
            action_id: receipt.operation().action_id(),
            patch_id: mutation.patch_identity(),
            snapshot,
            identity,
            manifest,
            artifact,
            receipt,
        })
    }
}

const fn candidate_publication_error(
    gateway: &mut WorkspaceGateway,
    failure: &SnapshotPublicationFailure,
) -> WorkspaceError {
    let compensated = failure.compensation_failure().is_none();
    gateway.workspace_mut().state_mut().set_condition(if compensated {
        WorkspaceCondition::Dirty
    } else {
        WorkspaceCondition::Indeterminate
    });
    candidate_error(
        if compensated { ErrorCode::Artifact } else { ErrorCode::Git },
        RecoveryClass::Reconcile,
        if compensated {
            "candidate manifest was not finalized; its retained snapshot was released"
        } else {
            "candidate manifest failed and retained snapshot cleanup was inconclusive"
        },
    )
}

/// Returns canonical payload bytes for a candidate action intent.
#[must_use]
pub fn candidate_authorization_payload(
    mutation: &MutationOutcome,
    snapshot_id: SnapshotId,
) -> Vec<u8> {
    candidate_payload(mutation, snapshot_id, None)
}

/// Returns canonical candidate authority bytes for a durable mutation handoff.
#[must_use]
pub fn candidate_authorization_payload_for_reference(
    mutation: MutationOutcomeReference,
    snapshot_id: SnapshotId,
) -> Vec<u8> {
    candidate_payload_reference(mutation, snapshot_id, None)
}

/// Returns the canonical candidate payload for an exact predicted patch outcome.
///
/// This is inert preparation for independently obtaining the candidate authorization before an
/// atomic caller enters a patch-then-candidate flow. C1 still reconstructs the payload from the
/// actual [`MutationOutcome`] and rejects any disagreement before candidate creation.
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "the complete predicted mutation identity remains explicit"
)]
pub fn predicted_candidate_authorization_payload(
    patch_action_id: ActionId,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: Generation,
    revision: RevisionNumber,
    patch_id: PatchIdentity,
    snapshot_id: SnapshotId,
) -> Vec<u8> {
    candidate_payload_fields(
        patch_action_id,
        workspace_id,
        resource_id,
        generation,
        revision,
        patch_id,
        snapshot_id,
        None,
    )
}

/// Returns canonical candidate payload bytes bound to one exact validated C4 caller.
#[must_use]
pub fn candidate_authorization_payload_for_caller(
    mutation: &MutationOutcome,
    snapshot_id: SnapshotId,
    caller: &crate::WorkspaceCallerBinding,
) -> Vec<u8> {
    candidate_payload(mutation, snapshot_id, Some(caller))
}

/// Returns canonical candidate authority bytes bound to a caller and durable mutation handoff.
#[must_use]
pub fn candidate_authorization_payload_for_reference_and_caller(
    mutation: MutationOutcomeReference,
    snapshot_id: SnapshotId,
    caller: &crate::WorkspaceCallerBinding,
) -> Vec<u8> {
    candidate_payload_reference(mutation, snapshot_id, Some(caller))
}

fn candidate_payload(
    mutation: &MutationOutcome,
    snapshot_id: SnapshotId,
    caller: Option<&crate::WorkspaceCallerBinding>,
) -> Vec<u8> {
    candidate_payload_reference(mutation.reference(), snapshot_id, caller)
}

fn candidate_payload_reference(
    mutation: MutationOutcomeReference,
    snapshot_id: SnapshotId,
    caller: Option<&crate::WorkspaceCallerBinding>,
) -> Vec<u8> {
    let reference = mutation.canonical_bytes();
    let mut bytes = if caller.is_some() {
        b"PERITUS-WORKSPACE-CANDIDATE-REFERENCE-V2\0".to_vec()
    } else {
        b"PERITUS-WORKSPACE-CANDIDATE-REFERENCE-V1\0".to_vec()
    };
    let reference_length =
        u64::try_from(reference.len()).expect("canonical mutation outcome length fits u64");
    bytes.extend_from_slice(&reference_length.to_be_bytes());
    bytes.extend_from_slice(&reference);
    bytes.extend_from_slice(snapshot_id.as_bytes());
    if let Some(caller) = caller {
        crate::caller::append_caller(&mut bytes, Some(caller));
    }
    bytes
}

#[allow(
    clippy::too_many_arguments,
    reason = "the canonical candidate authority identity has seven independent fields"
)]
fn candidate_payload_fields(
    patch_action_id: ActionId,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: Generation,
    revision: RevisionNumber,
    patch_id: PatchIdentity,
    snapshot_id: SnapshotId,
    caller: Option<&crate::WorkspaceCallerBinding>,
) -> Vec<u8> {
    let mut bytes = if caller.is_some() {
        b"PERITUS-WORKSPACE-CANDIDATE-V2\0".to_vec()
    } else {
        b"PERITUS-WORKSPACE-CANDIDATE-V1\0".to_vec()
    };
    bytes.extend_from_slice(patch_action_id.as_bytes());
    bytes.extend_from_slice(workspace_id.as_bytes());
    bytes.extend_from_slice(resource_id.as_bytes());
    bytes.extend_from_slice(&generation.get().to_be_bytes());
    bytes.extend_from_slice(&revision.get().to_be_bytes());
    bytes.extend_from_slice(patch_id.as_bytes());
    bytes.extend_from_slice(snapshot_id.as_bytes());
    if let Some(caller) = caller {
        crate::caller::append_caller(&mut bytes, Some(caller));
    }
    bytes
}

fn validate_mutation_input(
    state: &crate::WorkspaceState,
    mutation: MutationOutcomeReference,
) -> Result<(), WorkspaceError> {
    if mutation.workspace_id() != state.binding().workspace_id()
        || mutation.resource_id() != state.binding().resource_id()
    {
        return Err(candidate_error(
            ErrorCode::ResourceMismatch,
            RecoveryClass::CorrectRequest,
            "patch outcome belongs to another exact workspace resource",
        ));
    }
    if mutation.generation() != state.generation() || mutation.revision() != state.revision() {
        return Err(candidate_error(
            ErrorCode::StaleWorkspace,
            RecoveryClass::Reauthorize,
            "patch outcome differs from current workspace counters",
        ));
    }
    Ok(())
}

fn combined_detail(
    left: peritus_types::Sha256Digest,
    right: peritus_types::Sha256Digest,
) -> peritus_types::Sha256Digest {
    let mut bytes = b"PERITUS-WORKSPACE-CANDIDATE-DETAIL-V1\0".to_vec();
    bytes.extend_from_slice(left.as_bytes());
    bytes.extend_from_slice(right.as_bytes());
    peritus_codec::sha256(&bytes)
}

fn candidate_result_bytes(
    patch: PatchIdentity,
    snapshot: &CandidateSnapshot,
    manifest: &WorkspaceManifest,
    artifact: ArtifactDigest,
) -> Vec<u8> {
    let mut bytes = CANDIDATE_RESULT_MAGIC.to_vec();
    bytes.extend_from_slice(patch.as_bytes());
    put_result_bytes(&mut bytes, snapshot.manifest().bytes());
    put_result_bytes(&mut bytes, manifest.canonical_bytes());
    bytes.extend_from_slice(artifact.as_bytes());
    bytes
}

fn put_result_bytes(target: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("bounded candidate result length fits u64");
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
}

const fn candidate_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> WorkspaceError {
    WorkspaceError::new(code, WorkspaceOperation::Candidate, recovery, detail)
}
