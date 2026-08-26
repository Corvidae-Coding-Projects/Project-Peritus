//! Closed CAS-fenced production-pointer commands.

use crate::{
    ActivationAuthorization, EvolutionError, EvolutionErrorKind, EvolutionLimits,
    EvolutionOperation, EvolutionRecovery, ProductionHarnessBinding, PromotionId,
    PromotionPolicyBinding, PromotionProposal, RollbackId, RollbackProposal,
    identity::digest_parts,
};
use peritus_types::{CommandId, EventId, ProjectId, Sha256Digest};

/// Closed pointer command semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerCommandKind {
    /// Establishes the first exact production pointer.
    InitializeProductionHarness {
        /// Exact installed E1 production binding.
        initial: ProductionHarnessBinding,
        /// Protected typed promotion policy.
        policy: PromotionPolicyBinding,
        /// Caller-tightened pointer bounds.
        limits: EvolutionLimits,
        /// Finalized initialization artifact.
        evidence_artifact: Sha256Digest,
        /// Exact initialization evidence digest.
        evidence_digest: Sha256Digest,
    },
    /// Fences one selected exact promotion against the current pointer.
    PreparePromotion(PromotionProposal),
    /// Activates one prepared promotion with exact consumed authority.
    ActivatePromotion {
        /// Prepared proposal identity.
        promotion_id: PromotionId,
        /// Atomic campaign-terminal checkpoint digest.
        campaign_terminal_digest: Sha256Digest,
        /// Already-validated B0/B1 facts.
        authorization: ActivationAuthorization,
    },
    /// Fences one exact known-target rollback against the current pointer.
    PrepareRollback(RollbackProposal),
    /// Activates one prepared rollback with new exact consumed authority.
    ActivateRollback {
        /// Prepared rollback identity.
        rollback_id: RollbackId,
        /// Already-validated B0/B1 facts.
        authorization: ActivationAuthorization,
    },
    /// Clears a prepared action after an explicit denial/recovery decision.
    CancelPending {
        /// Stable digest of the cancellation reason evidence.
        reason_digest: Sha256Digest,
    },
}

/// One exact production-pointer command with head and generation fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerCommand {
    command_id: CommandId,
    event_id: EventId,
    project_id: ProjectId,
    expected_sequence: u64,
    expected_head: Option<EventId>,
    expected_generation: u64,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    kind: PointerCommandKind,
    digest: Sha256Digest,
}

impl PointerCommand {
    /// Constructs a complete fenced pointer command.
    ///
    /// # Errors
    /// Rejects inconsistent genesis/head/generation or initialization policy binding.
    #[allow(clippy::too_many_arguments, reason = "all pointer CAS facts remain explicit")]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        project_id: ProjectId,
        expected_sequence: u64,
        expected_head: Option<EventId>,
        expected_generation: u64,
        prior_state_digest: Sha256Digest,
        policy_digest: Sha256Digest,
        kind: PointerCommandKind,
    ) -> Result<Self, EvolutionError> {
        if (expected_sequence == 0) != expected_head.is_none()
            || (expected_sequence == 0) != (expected_generation == 0)
            || matches!(&kind, PointerCommandKind::InitializeProductionHarness { policy, .. }
                if policy.digest() != policy_digest)
        {
            return Err(invalid());
        }
        let digest = pointer_command_digest(
            command_id,
            event_id,
            project_id,
            expected_sequence,
            expected_head,
            expected_generation,
            prior_state_digest,
            policy_digest,
            &kind,
        );
        Ok(Self {
            command_id,
            event_id,
            project_id,
            expected_sequence,
            expected_head,
            expected_generation,
            prior_state_digest,
            policy_digest,
            kind,
            digest,
        })
    }
    /// Command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Reserved event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Aggregate/project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Expected current semantic-event sequence.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Expected aggregate head.
    #[must_use]
    pub const fn expected_head(&self) -> Option<EventId> {
        self.expected_head
    }
    /// Expected current pointer generation.
    #[must_use]
    pub const fn expected_generation(&self) -> u64 {
        self.expected_generation
    }
    /// Expected complete prior checkpoint digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Frozen protected policy-binding digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Exact command semantics.
    #[must_use]
    pub const fn kind(&self) -> &PointerCommandKind {
        &self.kind
    }
    /// Digest of every fence and semantic field.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[allow(clippy::too_many_arguments)]
fn pointer_command_digest(
    command: CommandId,
    event: EventId,
    project: ProjectId,
    sequence: u64,
    head: Option<EventId>,
    generation: u64,
    prior: Sha256Digest,
    policy: Sha256Digest,
    kind: &PointerCommandKind,
) -> Sha256Digest {
    let sequence = sequence.to_be_bytes();
    let generation = generation.to_be_bytes();
    let semantic = semantic_digest(kind);
    digest_parts(
        b"peritus.f0.pointer-command.v1\0",
        &[
            command.as_bytes(),
            event.as_bytes(),
            project.as_bytes(),
            &sequence,
            head.as_ref().map_or(&[][..], |value| value.as_bytes()),
            &generation,
            prior.as_bytes(),
            policy.as_bytes(),
            semantic.as_bytes(),
        ],
    )
}

pub(crate) fn semantic_digest(kind: &PointerCommandKind) -> Sha256Digest {
    let mut bytes = Vec::new();
    match kind {
        PointerCommandKind::InitializeProductionHarness {
            initial,
            policy,
            limits,
            evidence_artifact,
            evidence_digest,
        } => {
            bytes.push(1);
            bytes.extend_from_slice(initial.digest().as_bytes());
            bytes.extend_from_slice(policy.digest().as_bytes());
            bytes.extend_from_slice(limits.digest().as_bytes());
            bytes.extend_from_slice(evidence_artifact.as_bytes());
            bytes.extend_from_slice(evidence_digest.as_bytes());
        }
        PointerCommandKind::PreparePromotion(value) => append(&mut bytes, 2, value.digest()),
        PointerCommandKind::ActivatePromotion {
            promotion_id,
            campaign_terminal_digest,
            authorization,
        } => {
            bytes.push(3);
            bytes.extend_from_slice(promotion_id.as_bytes());
            bytes.extend_from_slice(campaign_terminal_digest.as_bytes());
            bytes.extend_from_slice(authorization.digest().as_bytes());
        }
        PointerCommandKind::PrepareRollback(value) => append(&mut bytes, 4, value.digest()),
        PointerCommandKind::ActivateRollback { rollback_id, authorization } => {
            bytes.push(5);
            bytes.extend_from_slice(rollback_id.as_bytes());
            bytes.extend_from_slice(authorization.digest().as_bytes());
        }
        PointerCommandKind::CancelPending { reason_digest } => {
            append(&mut bytes, 6, *reason_digest);
        }
    }
    digest_parts(b"peritus.f0.pointer-command-kind.v1\0", &[&bytes])
}

fn append(bytes: &mut Vec<u8>, tag: u8, digest: Sha256Digest) {
    bytes.push(tag);
    bytes.extend_from_slice(digest.as_bytes());
}

const fn invalid() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::InvalidInput,
        EvolutionOperation::TransitionPointer,
        EvolutionRecovery::CorrectInput,
        "pointer command fence, generation, or policy binding is inconsistent",
    )
}
