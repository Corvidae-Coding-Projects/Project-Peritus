//! Authorized candidate creation and content-addressed manifest finalization.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, FinalizedArtifact};
use peritus_git::{CandidateRequest, CandidateSnapshot, SnapshotRequest};
use peritus_patch::PatchIdentity;
use peritus_types::{ActionId, Generation, ResourceId, RevisionNumber, SnapshotId, WorkspaceId};

use crate::{
    ErrorCode, MutationOutcome, RecoveryClass, SnapshotIdentity, WorkspaceAuthorizationRequest,
    WorkspaceCondition, WorkspaceError, WorkspaceGateway, WorkspaceManifest, WorkspaceOperation,
};
use crate::{SnapshotPublicationFailure, finalize_snapshot_manifest};

/// Retained immutable candidate and its finalized C0 artifact observation.
pub struct CandidateOutcome {
    action_id: ActionId,
    patch_id: PatchIdentity,
    snapshot: CandidateSnapshot,
    identity: SnapshotIdentity,
    manifest: WorkspaceManifest,
    artifact: FinalizedArtifact,
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
}

impl WorkspaceGateway {
    /// Reconciles the exact applied patch, creates a Git tree and successor snapshot, finalizes its
    /// manifest, and only then marks the logical workspace revision clean.
    ///
    /// # Errors
    ///
    /// On any Git or artifact failure, the live workspace remains dirty and requires inspection.
    pub fn create_candidate(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        mutation: &MutationOutcome,
        snapshot_id: SnapshotId,
        artifacts: &ArtifactStore,
    ) -> Result<CandidateOutcome, WorkspaceError> {
        validate_mutation_input(self.state(), mutation)?;
        let payload = candidate_payload(mutation, snapshot_id, authorization.caller_binding());
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
            mutation.applied_patch().manifest_digest(),
            candidate.manifest_digest(),
        );
        let manifest = WorkspaceManifest::candidate(
            prior.workspace_id(),
            prior.generation(),
            prior.revision(),
            next_revision,
            permit.action_id(),
            permit.action_digest(),
            snapshot.tree(),
            detail_digest,
        );
        let artifact = finalize_snapshot_manifest(
            &repository,
            &snapshot,
            &manifest,
            artifacts,
            permit.dispatch_event(),
        )
        .map_err(|failure| candidate_publication_error(self, &failure))?;
        self.workspace_mut().state_mut().install(identity.clone());
        Ok(CandidateOutcome {
            action_id: permit.action_id(),
            patch_id: mutation.patch_identity(),
            snapshot,
            identity,
            manifest,
            artifact,
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

fn candidate_payload(
    mutation: &MutationOutcome,
    snapshot_id: SnapshotId,
    caller: Option<&crate::WorkspaceCallerBinding>,
) -> Vec<u8> {
    candidate_payload_fields(
        mutation.action_id(),
        mutation.workspace_id(),
        mutation.resource_id(),
        mutation.generation(),
        mutation.revision(),
        mutation.patch_identity(),
        snapshot_id,
        caller,
    )
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
    mutation: &MutationOutcome,
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

const fn candidate_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> WorkspaceError {
    WorkspaceError::new(code, WorkspaceOperation::Candidate, recovery, detail)
}
