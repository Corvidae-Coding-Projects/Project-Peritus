//! Exact authorization boundary facts and auditable rollback proposals.

use crate::{
    ActivationId, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    ProductionHarnessBinding, ProductionHarnessState, RollbackId, identity::digest_parts,
};
use peritus_types::{ProjectId, Sha256Digest};

/// Checked B0/B1 boundary facts supplied by the effectful authority adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivationAuthorization {
    action_digest: Sha256Digest,
    dispatch_digest: Sha256Digest,
    capability_use_digest: Sha256Digest,
    approval_use_digest: Sha256Digest,
    authority_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl ActivationAuthorization {
    /// Captures exact already-validated authority facts for one action digest.
    #[must_use]
    pub fn new(
        action_digest: Sha256Digest,
        dispatch_digest: Sha256Digest,
        capability_use_digest: Sha256Digest,
        approval_use_digest: Sha256Digest,
        authority_digest: Sha256Digest,
    ) -> Self {
        let digest = digest_parts(
            b"peritus.f0.activation-authorization.v1\0",
            &[
                action_digest.as_bytes(),
                dispatch_digest.as_bytes(),
                capability_use_digest.as_bytes(),
                approval_use_digest.as_bytes(),
                authority_digest.as_bytes(),
            ],
        );
        Self {
            action_digest,
            dispatch_digest,
            capability_use_digest,
            approval_use_digest,
            authority_digest,
            digest,
        }
    }
    /// Exact promotion or rollback action digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest {
        self.action_digest
    }
    /// Exact B0 dispatched action digest.
    #[must_use]
    pub const fn dispatch_digest(self) -> Sha256Digest {
        self.dispatch_digest
    }
    /// Durably committed capability-use digest.
    #[must_use]
    pub const fn capability_use_digest(self) -> Sha256Digest {
        self.capability_use_digest
    }
    /// Consumed approve-once use digest.
    #[must_use]
    pub const fn approval_use_digest(self) -> Sha256Digest {
        self.approval_use_digest
    }
    /// Authority epoch/registry binding digest.
    #[must_use]
    pub const fn authority_digest(self) -> Sha256Digest {
        self.authority_digest
    }
    /// Digest of every checked boundary fact.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

/// Exact E1/schema compatibility observation for one proposed rollback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityWitness {
    current_digest: Sha256Digest,
    target_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
    compatible: bool,
}

impl CompatibilityWitness {
    /// Creates one explicit checked-boundary compatibility result.
    #[must_use]
    pub const fn new(
        current: ProductionHarnessBinding,
        target: ProductionHarnessBinding,
        policy_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
        compatible: bool,
    ) -> Self {
        Self {
            current_digest: current.digest(),
            target_digest: target.digest(),
            policy_digest,
            evidence_digest,
            compatible,
        }
    }
    /// Current pointer digest.
    #[must_use]
    pub const fn current_digest(self) -> Sha256Digest {
        self.current_digest
    }
    /// Target pointer digest.
    #[must_use]
    pub const fn target_digest(self) -> Sha256Digest {
        self.target_digest
    }
    /// Frozen protected policy-binding digest.
    #[must_use]
    pub const fn policy_digest(self) -> Sha256Digest {
        self.policy_digest
    }
    /// Exact compatibility evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    /// Whether the owning boundary observed compatible schemas.
    #[must_use]
    pub const fn compatible(self) -> bool {
        self.compatible
    }
}

/// One append-only rollback action targeting a retained prior activation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollbackProposal {
    id: RollbackId,
    project_id: ProjectId,
    current: ProductionHarnessBinding,
    target: ProductionHarnessBinding,
    target_activation: ActivationId,
    rollback_of: ActivationId,
    policy_digest: Sha256Digest,
    compatibility_evidence_digest: Sha256Digest,
    evidence_bundle_artifact: Sha256Digest,
    digest: Sha256Digest,
}

impl RollbackProposal {
    /// Constructs a rollback to one retained, distinct, compatible former pointer.
    ///
    /// # Errors
    /// Rejects unknown targets, the current pointer, policy drift, or incompatibility.
    pub fn new(
        state: &ProductionHarnessState,
        target: ProductionHarnessBinding,
        target_activation: ActivationId,
        witness: CompatibilityWitness,
        evidence_bundle_artifact: Sha256Digest,
    ) -> Result<Self, EvolutionError> {
        let current = state.current();
        let rollback_of =
            state.history().last().map(ActivationRecordRef::id).ok_or_else(illegal)?;
        if current == target
            || witness.current_digest() != current.digest()
            || witness.target_digest() != target.digest()
            || witness.policy_digest() != state.policy().digest()
            || !witness.compatible()
            || !state
                .history()
                .iter()
                .any(|record| record.id() == target_activation && record.successor() == target)
        {
            return Err(illegal());
        }
        let digest = digest_parts(
            b"peritus.f0.rollback-proposal.v1\0",
            &[
                state.project_id().as_bytes(),
                current.digest().as_bytes(),
                target.digest().as_bytes(),
                target_activation.as_bytes(),
                rollback_of.as_bytes(),
                state.policy().digest().as_bytes(),
                witness.evidence_digest().as_bytes(),
                evidence_bundle_artifact.as_bytes(),
            ],
        );
        Ok(Self {
            id: RollbackId::derive(b"peritus.f0.rollback-id.v1\0", digest),
            project_id: state.project_id(),
            current,
            target,
            target_activation,
            rollback_of,
            policy_digest: state.policy().digest(),
            compatibility_evidence_digest: witness.evidence_digest(),
            evidence_bundle_artifact,
            digest,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "every persisted rollback action fact stays explicit"
    )]
    pub(crate) fn from_exact_parts(
        project_id: ProjectId,
        current: ProductionHarnessBinding,
        target: ProductionHarnessBinding,
        target_activation: ActivationId,
        rollback_of: ActivationId,
        policy_digest: Sha256Digest,
        compatibility_evidence_digest: Sha256Digest,
        evidence_bundle_artifact: Sha256Digest,
    ) -> Result<Self, EvolutionError> {
        let equal_pointer = current == target;
        let reverses_target = target_activation == rollback_of;
        if equal_pointer || reverses_target {
            return Err(illegal());
        }
        let digest = digest_parts(
            b"peritus.f0.rollback-proposal.v1\0",
            &[
                project_id.as_bytes(),
                current.digest().as_bytes(),
                target.digest().as_bytes(),
                target_activation.as_bytes(),
                rollback_of.as_bytes(),
                policy_digest.as_bytes(),
                compatibility_evidence_digest.as_bytes(),
                evidence_bundle_artifact.as_bytes(),
            ],
        );
        Ok(Self {
            id: RollbackId::derive(b"peritus.f0.rollback-id.v1\0", digest),
            project_id,
            current,
            target,
            target_activation,
            rollback_of,
            policy_digest,
            compatibility_evidence_digest,
            evidence_bundle_artifact,
            digest,
        })
    }
    /// Rollback action identity.
    #[must_use]
    pub const fn id(&self) -> RollbackId {
        self.id
    }
    /// Project authority identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Exact fenced current pointer.
    #[must_use]
    pub const fn current(&self) -> ProductionHarnessBinding {
        self.current
    }
    /// Exact retained target pointer.
    #[must_use]
    pub const fn target(&self) -> ProductionHarnessBinding {
        self.target
    }
    /// Activation which first established the target.
    #[must_use]
    pub const fn target_activation(&self) -> ActivationId {
        self.target_activation
    }
    /// Current activation being reversed.
    #[must_use]
    pub const fn rollback_of(&self) -> ActivationId {
        self.rollback_of
    }
    /// Frozen protected policy-binding digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Exact E1/schema compatibility evidence digest.
    #[must_use]
    pub const fn compatibility_evidence_digest(&self) -> Sha256Digest {
        self.compatibility_evidence_digest
    }
    /// Finalized rollback evidence-bundle artifact.
    #[must_use]
    pub const fn evidence_bundle_artifact(&self) -> Sha256Digest {
        self.evidence_bundle_artifact
    }
    /// Digest of the complete rollback action.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

trait ActivationRecordRef {
    fn id(&self) -> ActivationId;
}
impl ActivationRecordRef for crate::ActivationRecord {
    fn id(&self) -> ActivationId {
        self.id()
    }
}

const fn illegal() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::PolicyRejected,
        EvolutionOperation::Rollback,
        EvolutionRecovery::CorrectInput,
        "rollback target is current, unknown, incompatible, or policy-drifted",
    )
}
