//! Exact-authority restart recovery for one registered-folder patch.

use std::{fs, path::PathBuf};

use peritus_patch::{PatchIdentity, PatchSet, RecoveryBinding, RecoveryState};
use peritus_types::{
    ActionId, ActorId, EnvironmentId, ResourceId, RevisionNumber, Sha256Digest, WorkspaceId,
};

use crate::{
    ErrorCode, FolderIdentity, RecoveryClass, WorkspaceError, WorkspaceOperation, consumption,
    transaction_namespace::{binding_manifest_path, is_canonical_transaction_directory},
};

use super::binding::{folder_patch_action_digest, folder_patch_payload_for, revalidate_identity};

/// Durable action-marker relationship proven before any recovery mutation is attempted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FolderMutationActionMarker {
    /// The exact action has no consumption marker in this revision ledger.
    Missing,
    /// The marker binds the exact reconstructed canonical folder-patch action.
    Exact,
    /// The action identity exists but its canonical digest differs.
    DigestMismatch,
}

/// Conservative classification of the exact C1 recovery attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FolderMutationRecoveryState {
    /// Neither the action marker nor a transaction exists; no C1 attempt reached consumption.
    NoAttempt,
    /// The exact action was consumed, but its transaction directory is no longer present.
    ConsumedWithoutTransaction,
    /// The exact transaction was durably installed and its metadata was reconciled.
    AlreadyApplied,
    /// The exact transaction was returned to all declared preimages.
    RolledBackCleanly,
    /// At least one target matches neither the transaction's preimage nor postimage.
    Dirty,
    /// Authority, namespace, metadata, or cleanup state did not prove one safe conclusion.
    Indeterminate,
}

/// Unprivileged exact facts required to recover one previously authorized folder mutation.
pub struct FolderMutationRecoveryRequest {
    identity: FolderIdentity,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    actor_id: ActorId,
    action_id: ActionId,
    transaction_root: PathBuf,
    patch: PatchSet,
}

impl FolderMutationRecoveryRequest {
    /// Binds recovery to the original actor, action, target, revision, and exact patch.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "recovery authority must retain every independent original-action binding"
    )]
    pub fn new(
        identity: FolderIdentity,
        resource_id: ResourceId,
        environment_id: EnvironmentId,
        actor_id: ActorId,
        action_id: ActionId,
        transaction_root: impl Into<PathBuf>,
        patch: PatchSet,
    ) -> Self {
        Self {
            identity,
            resource_id,
            environment_id,
            actor_id,
            action_id,
            transaction_root: transaction_root.into(),
            patch,
        }
    }
}

/// Typed evidence returned after C1 has safely inspected or reconciled one exact transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderMutationRecoveryOutcome {
    state: FolderMutationRecoveryState,
    marker: FolderMutationActionMarker,
    action_id: ActionId,
    action_digest: Sha256Digest,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: peritus_types::Generation,
    revision: RevisionNumber,
    patch_identity: PatchIdentity,
    observed_binding: Option<RecoveryBinding>,
    observed_identity: Option<PatchIdentity>,
    quarantined: bool,
    cleanup_pending: bool,
}

impl FolderMutationRecoveryOutcome {
    /// Returns the conservative recovery classification.
    #[must_use]
    pub const fn state(&self) -> FolderMutationRecoveryState {
        self.state
    }
    /// Returns how the durable action marker relates to the reconstructed authority.
    #[must_use]
    pub const fn marker(&self) -> FolderMutationActionMarker {
        self.marker
    }
    /// Returns the exact action identity reconstructed by C1.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the canonical action digest required by the durable marker.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        self.action_digest
    }
    /// Returns the bound workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the exact registered resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    /// Returns the fenced workspace generation.
    #[must_use]
    pub const fn generation(&self) -> peritus_types::Generation {
        self.generation
    }
    /// Returns the fenced logical revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// Returns the exact patch identity recovery was allowed to inspect.
    #[must_use]
    pub const fn patch_identity(&self) -> PatchIdentity {
        self.patch_identity
    }
    /// Returns a decoded manifest binding, when safely available.
    #[must_use]
    pub const fn observed_binding(&self) -> Option<RecoveryBinding> {
        self.observed_binding
    }
    /// Returns a decoded manifest patch identity, when safely available.
    #[must_use]
    pub const fn observed_identity(&self) -> Option<PatchIdentity> {
        self.observed_identity
    }
    /// Reports that corrupt metadata was quarantined from the active namespace.
    #[must_use]
    pub const fn quarantined(&self) -> bool {
        self.quarantined
    }
    /// Reports that a conclusive transaction still has pending cleanup.
    #[must_use]
    pub const fn cleanup_pending(&self) -> bool {
        self.cleanup_pending
    }
}

/// Recovers only a transaction previously authorized and consumed by this exact C1 boundary.
///
/// Missing, mismatched, or ambiguous authority never permits transaction recovery to touch target
/// bytes. A consumed marker with no transaction is reported distinctly because G0 must reconcile
/// it against retained pre/postimage checkpoints rather than infer success.
///
/// # Errors
/// Rejects changed folder identity, invalid patch binding, or an unsafe transaction namespace.
pub fn recover_folder_mutation(
    request: FolderMutationRecoveryRequest,
) -> Result<FolderMutationRecoveryOutcome, WorkspaceError> {
    let FolderMutationRecoveryRequest {
        identity,
        resource_id,
        environment_id,
        actor_id,
        action_id,
        transaction_root,
        patch,
    } = request;
    revalidate_identity(&identity, WorkspaceOperation::Reconcile)?;
    let workspace_id = patch.workspace_id();
    let generation = patch.expected_generation();
    let revision = patch.expected_revision();
    let namespace = crate::transaction_namespace::open_folder(
        transaction_root,
        workspace_id,
        resource_id,
        environment_id,
        identity.digest(),
        identity.root(),
    )?;
    let binding = consumption::ActionConsumptionBinding::new(
        workspace_id,
        resource_id,
        environment_id,
        generation,
        revision,
    );
    let actions = consumption::restore(&namespace, binding)?;
    let payload = folder_patch_payload_for(
        workspace_id,
        resource_id,
        environment_id,
        identity.digest(),
        generation,
        revision,
        &patch,
    );
    let action_digest =
        folder_patch_action_digest(actor_id, action_id, environment_id, resource_id, payload)?;
    let marker = match actions.get(&action_id) {
        None => FolderMutationActionMarker::Missing,
        Some(observed) if *observed == action_digest => FolderMutationActionMarker::Exact,
        Some(_) => FolderMutationActionMarker::DigestMismatch,
    };
    let expected_identity = patch.identity();
    let transaction = exact_transaction(&namespace, expected_identity)?;
    let base = RecoveryEvidence {
        marker,
        action_id,
        action_digest,
        workspace_id,
        resource_id,
        generation,
        revision,
        patch_identity: expected_identity,
    };
    let Some(transaction) = transaction else {
        return Ok(base.outcome(
            match marker {
                FolderMutationActionMarker::Missing => FolderMutationRecoveryState::NoAttempt,
                FolderMutationActionMarker::Exact => {
                    FolderMutationRecoveryState::ConsumedWithoutTransaction
                }
                FolderMutationActionMarker::DigestMismatch => {
                    FolderMutationRecoveryState::Indeterminate
                }
            },
            None,
        ));
    };
    if marker != FolderMutationActionMarker::Exact {
        return Ok(base.outcome(FolderMutationRecoveryState::Indeterminate, None));
    }
    let expected_binding = RecoveryBinding::new(workspace_id, generation, revision);
    let Ok(recovered) =
        peritus_patch::recover_transaction(identity.root(), transaction, expected_binding)
    else {
        return Ok(base.outcome(FolderMutationRecoveryState::Indeterminate, None));
    };
    let state = match recovered.state() {
        RecoveryState::AlreadyApplied => FolderMutationRecoveryState::AlreadyApplied,
        RecoveryState::RolledBackCleanly => FolderMutationRecoveryState::RolledBackCleanly,
        RecoveryState::Dirty => FolderMutationRecoveryState::Dirty,
        RecoveryState::Indeterminate => FolderMutationRecoveryState::Indeterminate,
    };
    Ok(base.outcome(state, Some(&recovered)))
}

struct RecoveryEvidence {
    marker: FolderMutationActionMarker,
    action_id: ActionId,
    action_digest: Sha256Digest,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: peritus_types::Generation,
    revision: RevisionNumber,
    patch_identity: PatchIdentity,
}

impl RecoveryEvidence {
    fn outcome(
        self,
        state: FolderMutationRecoveryState,
        recovered: Option<&peritus_patch::RecoveryOutcome>,
    ) -> FolderMutationRecoveryOutcome {
        FolderMutationRecoveryOutcome {
            state,
            marker: self.marker,
            action_id: self.action_id,
            action_digest: self.action_digest,
            workspace_id: self.workspace_id,
            resource_id: self.resource_id,
            generation: self.generation,
            revision: self.revision,
            patch_identity: self.patch_identity,
            observed_binding: recovered.and_then(peritus_patch::RecoveryOutcome::binding),
            observed_identity: recovered.and_then(peritus_patch::RecoveryOutcome::identity),
            quarantined: recovered.is_some_and(peritus_patch::RecoveryOutcome::quarantined),
            cleanup_pending: recovered.is_some_and(peritus_patch::RecoveryOutcome::cleanup_pending),
        }
    }
}

fn exact_transaction(
    namespace: &std::path::Path,
    identity: PatchIdentity,
) -> Result<Option<PathBuf>, WorkspaceError> {
    let expected = namespace.join(format!("txn-{}", identity.to_hex()));
    let mut found = None;
    for entry in fs::read_dir(namespace)
        .map_err(|_| recovery_error("folder transaction namespace cannot be inspected"))?
    {
        let entry = entry
            .map_err(|_| recovery_error("folder transaction namespace cannot be inspected"))?;
        let path = entry.path();
        if path == binding_manifest_path(namespace)
            || path == consumption::action_ledger_root(namespace)
        {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|_| recovery_error("folder transaction entry type cannot be inspected"))?;
        if path != expected
            || !is_canonical_transaction_directory(&path)
            || !kind.is_dir()
            || kind.is_symlink()
            || found.is_some()
        {
            return Err(recovery_error(
                "folder transaction namespace does not contain only the exact recovery target",
            ));
        }
        found = Some(path);
    }
    Ok(found)
}

const fn recovery_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
