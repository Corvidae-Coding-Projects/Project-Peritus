//! Complete long-lived project production-pointer checkpoint.

use crate::{
    ActivationAuthorization, ActivationId, EvolutionCampaignId, EvolutionLimits,
    ProductionHarnessBinding, PromotionPolicyBinding, PromotionProposal, RollbackProposal,
    identity::digest_parts,
};
use peritus_types::{EventId, ProjectId, Sha256Digest};

/// Production-pointer lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerPhase {
    /// One exact pointer is active.
    Active,
    /// An exact promotion proposal is fenced pending approval/activation.
    PromotionPending,
    /// An exact rollback proposal is fenced pending approval/activation.
    RollbackPending,
}

impl PointerPhase {
    pub(crate) const fn tag(self) -> u8 {
        self as u8
    }
}

/// Append-only activation classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationKind {
    /// Initial E1 production pointer.
    Initialization,
    /// Selected campaign promotion.
    Promotion,
    /// Newly approved rollback to a retained target.
    Rollback,
}

impl ActivationKind {
    const fn tag(self) -> u8 {
        self as u8
    }
}

/// One immutable pointer generation transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivationRecord {
    pub(crate) id: ActivationId,
    pub(crate) kind: ActivationKind,
    pub(crate) generation: u64,
    pub(crate) predecessor: Option<ProductionHarnessBinding>,
    pub(crate) successor: ProductionHarnessBinding,
    pub(crate) campaign_id: Option<EvolutionCampaignId>,
    pub(crate) action_digest: Sha256Digest,
    pub(crate) authorization: Option<ActivationAuthorization>,
    pub(crate) evidence_artifact: Sha256Digest,
    pub(crate) evidence_digest: Sha256Digest,
    pub(crate) rollback_of: Option<ActivationId>,
    pub(crate) digest: Sha256Digest,
}

impl ActivationRecord {
    /// Stable content-derived activation identity.
    #[must_use]
    pub const fn id(&self) -> ActivationId {
        self.id
    }
    /// Activation classification.
    #[must_use]
    pub const fn kind(&self) -> ActivationKind {
        self.kind
    }
    /// Positive pointer generation established by this record.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Prior exact pointer when one existed.
    #[must_use]
    pub const fn predecessor(&self) -> Option<ProductionHarnessBinding> {
        self.predecessor
    }
    /// Exact pointer established by this record.
    #[must_use]
    pub const fn successor(&self) -> ProductionHarnessBinding {
        self.successor
    }
    /// Campaign responsible for a promotion.
    #[must_use]
    pub const fn campaign_id(&self) -> Option<EvolutionCampaignId> {
        self.campaign_id
    }
    /// Exact promotion/rollback action digest.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        self.action_digest
    }
    /// Exact B0/B1 boundary facts when activation required authority.
    #[must_use]
    pub const fn authorization(&self) -> Option<ActivationAuthorization> {
        self.authorization
    }
    /// Finalized activation evidence artifact.
    #[must_use]
    pub const fn evidence_artifact(&self) -> Sha256Digest {
        self.evidence_artifact
    }
    /// Exact campaign-terminal or compatibility evidence digest.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
    /// Activation reversed by a rollback.
    #[must_use]
    pub const fn rollback_of(&self) -> Option<ActivationId> {
        self.rollback_of
    }
    /// Digest of every retained activation fact.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Exact activation prepared against the current pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingActivation {
    /// Exact selected campaign promotion.
    Promotion(PromotionProposal),
    /// Exact known-target rollback.
    Rollback(RollbackProposal),
}

impl PendingActivation {
    /// Exact inert action digest requiring authority.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest {
        match self {
            Self::Promotion(value) => value.digest(),
            Self::Rollback(value) => value.digest(),
        }
    }
}

/// Complete authoritative project production-pointer state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionHarnessState {
    pub(crate) project_id: ProjectId,
    pub(crate) current: ProductionHarnessBinding,
    pub(crate) policy: PromotionPolicyBinding,
    pub(crate) limits: EvolutionLimits,
    pub(crate) generation: u64,
    pub(crate) sequence: u64,
    pub(crate) last_event: EventId,
    pub(crate) state_digest: Sha256Digest,
    pub(crate) phase: PointerPhase,
    pub(crate) history: Vec<ActivationRecord>,
    pub(crate) pending: Option<PendingActivation>,
}

impl ProductionHarnessState {
    /// Project authority/aggregate identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Current exact production pointer.
    #[must_use]
    pub const fn current(&self) -> ProductionHarnessBinding {
        self.current
    }
    /// Current protected policy binding.
    #[must_use]
    pub const fn policy(&self) -> &PromotionPolicyBinding {
        &self.policy
    }
    /// Caller-tightened pointer bounds.
    #[must_use]
    pub const fn limits(&self) -> EvolutionLimits {
        self.limits
    }
    /// Monotonic pointer generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Applied semantic-event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Aggregate head event.
    #[must_use]
    pub const fn last_event(&self) -> EventId {
        self.last_event
    }
    /// Digest of the complete checkpoint.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Pointer lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> PointerPhase {
        self.phase
    }
    /// Canonical retained activation suffix in chronological order.
    #[must_use]
    pub fn history(&self) -> &[ActivationRecord] {
        &self.history
    }
    /// Exact prepared action.
    #[must_use]
    pub const fn pending(&self) -> Option<&PendingActivation> {
        self.pending.as_ref()
    }

    pub(crate) fn refresh_digest(&mut self) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.project_id.as_bytes());
        bytes.extend_from_slice(self.current.digest().as_bytes());
        bytes.extend_from_slice(self.policy.digest().as_bytes());
        bytes.extend_from_slice(self.limits.digest().as_bytes());
        bytes.extend_from_slice(&self.generation.to_be_bytes());
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(self.last_event.as_bytes());
        bytes.push(self.phase.tag());
        for record in &self.history {
            bytes.extend_from_slice(record.digest().as_bytes());
        }
        if let Some(pending) = &self.pending {
            bytes.push(1);
            bytes.extend_from_slice(pending.action_digest().as_bytes());
        } else {
            bytes.push(0);
        }
        self.state_digest = digest_parts(b"peritus.f0.production-pointer-state.v1\0", &[&bytes]);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn activation_record(
    kind: ActivationKind,
    generation: u64,
    predecessor: Option<ProductionHarnessBinding>,
    successor: ProductionHarnessBinding,
    campaign_id: Option<EvolutionCampaignId>,
    action_digest: Sha256Digest,
    authorization: Option<ActivationAuthorization>,
    evidence_artifact: Sha256Digest,
    evidence_digest: Sha256Digest,
    rollback_of: Option<ActivationId>,
) -> ActivationRecord {
    let mut bytes = Vec::new();
    bytes.push(kind.tag());
    bytes.extend_from_slice(&generation.to_be_bytes());
    bytes.push(u8::from(predecessor.is_some()));
    if let Some(value) = predecessor {
        bytes.extend_from_slice(value.digest().as_bytes());
    }
    bytes.extend_from_slice(successor.digest().as_bytes());
    bytes.push(u8::from(campaign_id.is_some()));
    if let Some(value) = campaign_id {
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes.extend_from_slice(action_digest.as_bytes());
    bytes.push(u8::from(authorization.is_some()));
    if let Some(value) = authorization {
        bytes.extend_from_slice(value.digest().as_bytes());
    }
    bytes.extend_from_slice(evidence_artifact.as_bytes());
    bytes.extend_from_slice(evidence_digest.as_bytes());
    bytes.push(u8::from(rollback_of.is_some()));
    if let Some(value) = rollback_of {
        bytes.extend_from_slice(value.as_bytes());
    }
    let digest = digest_parts(b"peritus.f0.activation-record.v1\0", &[&bytes]);
    ActivationRecord {
        id: ActivationId::derive(b"peritus.f0.activation-id.v1\0", digest),
        kind,
        generation,
        predecessor,
        successor,
        campaign_id,
        action_digest,
        authorization,
        evidence_artifact,
        evidence_digest,
        rollback_of,
        digest,
    }
}
