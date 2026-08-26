//! Exact artifact-to-`PatchSet` construction and C1 execution.

use peritus_artifact_store::ArtifactStore;
use peritus_patch::{FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage};
use peritus_types::{ActionId, ResourceId, SnapshotId};
use peritus_workspace::{
    WorkspaceAuthorizationRequest, WorkspaceGateway, candidate_authorization_payload,
    patch_authorization_payload, predicted_candidate_authorization_payload,
};

use crate::runtime::ArtifactReader;

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationPlan, MaterializationReceipt,
    MaterializationRecovery, PlannedFileOperation, WorkspaceSnapshot,
};

/// Exact action identities accompanying target-owned C1 authorization receipts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizationActions {
    patch: ActionId,
    candidate: ActionId,
}

impl AuthorizationActions {
    /// Constructs the expected patch and candidate actions.
    #[must_use]
    pub const fn new(patch: ActionId, candidate: ActionId) -> Self {
        Self { patch, candidate }
    }

    /// Returns the expected patch authorization action.
    #[must_use]
    pub const fn patch(self) -> ActionId {
        self.patch
    }

    /// Returns the expected candidate authorization action.
    #[must_use]
    pub const fn candidate(self) -> ActionId {
        self.candidate
    }
}

/// Exact inert B1/C1 authorization payloads for one deterministic materialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationAuthorizationPayloads {
    patch: Vec<u8>,
    candidate: Vec<u8>,
}

impl MaterializationAuthorizationPayloads {
    /// Borrows the exact patch action payload.
    #[must_use]
    pub fn patch(&self) -> &[u8] {
        &self.patch
    }

    /// Borrows the exact candidate action payload predicted from the same patch identity.
    #[must_use]
    pub fn candidate(&self) -> &[u8] {
        &self.candidate
    }
}

/// Complete successful effect-shell result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationResult {
    receipt: MaterializationReceipt,
}

/// Prepares the two inert authorization payloads required by [`execute_plan`].
///
/// The candidate payload is predicted from the exact deterministic patch identity. C1 later
/// reconstructs it from the actual patch outcome, so any drift still rejects before candidate
/// creation.
///
/// # Errors
/// Returns the same artifact or patch-construction failures as [`execute_plan`] before any effect.
pub fn materialization_authorization_payloads(
    plan: &MaterializationPlan,
    artifacts: &impl ArtifactReader,
    resource_id: ResourceId,
    actions: AuthorizationActions,
    snapshot_id: SnapshotId,
) -> Result<MaterializationAuthorizationPayloads, MaterializationError> {
    let patch = build_patch(plan, artifacts)?;
    Ok(MaterializationAuthorizationPayloads {
        patch: patch_authorization_payload(&patch),
        candidate: predicted_candidate_authorization_payload(
            actions.patch,
            plan.target().workspace_id(),
            resource_id,
            plan.target().generation(),
            plan.target().revision(),
            patch.identity(),
            snapshot_id,
        ),
    })
}

impl MaterializationResult {
    /// Borrows the exact durable receipt ready for its settlement command.
    #[must_use]
    pub const fn receipt(&self) -> &MaterializationReceipt {
        &self.receipt
    }

    /// Consumes the result and returns the receipt.
    #[must_use]
    pub fn into_receipt(self) -> MaterializationReceipt {
        self.receipt
    }
}

/// Executes one already-committed plan through C0 artifact reads and the C1 gateway.
///
/// # Errors
/// Returns before C1 mutation on artifact or patch construction disagreement. C1 failures retain
/// their non-success status and must be settled or reconciled by the durable driver.
#[allow(clippy::too_many_arguments, reason = "all authority and causal inputs remain explicit")]
pub fn execute_plan(
    plan: &MaterializationPlan,
    artifacts: &impl ArtifactReader,
    gateway: &mut WorkspaceGateway,
    patch_authorization: &WorkspaceAuthorizationRequest<'_>,
    candidate_authorization: &WorkspaceAuthorizationRequest<'_>,
    actions: AuthorizationActions,
    snapshot_id: SnapshotId,
    manifest_store: &ArtifactStore,
    started_at_millis: u64,
    completed_at_millis: u64,
) -> Result<MaterializationResult, MaterializationError> {
    require_target(plan.target(), gateway)?;
    let patch = build_patch(plan, artifacts)?;
    let patch_authorization_digest = peritus_codec::sha256(&patch_authorization_payload(&patch));
    let mutation =
        gateway.apply_patch(patch_authorization, patch).map_err(|error| workspace(&error))?;
    let candidate_authorization_digest =
        peritus_codec::sha256(&candidate_authorization_payload(&mutation, snapshot_id));
    let candidate = gateway
        .create_candidate(candidate_authorization, &mutation, snapshot_id, manifest_store)
        .map_err(|error| workspace(&error))?;
    let receipt = MaterializationReceipt::from_c1(
        plan,
        &mutation,
        &candidate,
        actions.patch,
        patch_authorization_digest,
        actions.candidate,
        candidate_authorization_digest,
        started_at_millis,
        completed_at_millis,
    )?;
    Ok(MaterializationResult { receipt })
}

fn build_patch(
    plan: &MaterializationPlan,
    artifacts: &impl ArtifactReader,
) -> Result<PatchSet, MaterializationError> {
    let mut operations = Vec::with_capacity(plan.operations().len());
    for operation in plan.operations() {
        match operation {
            PlannedFileOperation::Install {
                path,
                preimage,
                artifact_digest,
                byte_length,
                mode,
            } => {
                let artifact = artifacts.read_artifact(*artifact_digest, *byte_length)?;
                if artifact.digest() != *artifact_digest
                    || u64::try_from(artifact.bytes().len()).ok() != Some(*byte_length)
                {
                    return Err(artifact_mismatch());
                }
                let file =
                    FinalFile::new(artifact.bytes().to_vec(), *mode, LineEndingPolicy::Preserve)
                        .map_err(patch)?;
                if file.digest() != *artifact_digest || file.size() != *byte_length {
                    return Err(artifact_mismatch());
                }
                operations.push(if matches!(preimage, Preimage::Absent) {
                    PatchOperation::create(path.clone(), file)
                } else {
                    PatchOperation::replace(path.clone(), *preimage, file).map_err(patch)?
                });
            }
            PlannedFileOperation::Delete { path, preimage } => {
                operations.push(PatchOperation::delete(path.clone(), *preimage).map_err(patch)?);
            }
        }
    }
    PatchSet::new(
        plan.target().workspace_id(),
        plan.target().generation(),
        plan.target().revision(),
        operations,
    )
    .map_err(patch)
}

fn require_target(
    expected: &WorkspaceSnapshot,
    gateway: &WorkspaceGateway,
) -> Result<(), MaterializationError> {
    let current = WorkspaceSnapshot::from_c1(gateway.state().current_snapshot());
    if &current != expected {
        return Err(MaterializationError::new(
            MaterializationErrorKind::StaleWorkspace,
            MaterializationRecovery::Reobserve,
            "C1 workspace no longer matches the committed plan target",
        ));
    }
    Ok(())
}

fn artifact_mismatch() -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Artifact,
        MaterializationRecovery::Quarantine,
        "artifact bytes disagree with the committed output digest or size",
    )
}

fn patch(_error: peritus_patch::PatchError) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Patch,
        MaterializationRecovery::CorrectInput,
        "C1 patch construction rejected the committed plan",
    )
}

fn workspace(error: &peritus_workspace::WorkspaceError) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Workspace,
        match error.recovery() {
            peritus_workspace::RecoveryClass::CorrectRequest => {
                MaterializationRecovery::CorrectInput
            }
            peritus_workspace::RecoveryClass::Reauthorize => MaterializationRecovery::Reauthorize,
            peritus_workspace::RecoveryClass::Reobserve => MaterializationRecovery::Reobserve,
            peritus_workspace::RecoveryClass::Reconcile => MaterializationRecovery::Reconcile,
            peritus_workspace::RecoveryClass::Quarantine => MaterializationRecovery::Quarantine,
        },
        error.to_string(),
    )
}
