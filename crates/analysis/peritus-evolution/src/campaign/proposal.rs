//! Exact inert promotion and publication records.

use crate::{
    AttributionRecord, EvolutionCampaignId, EvolutionError, EvolutionErrorKind, EvolutionOperation,
    EvolutionRecovery, ProductionHarnessBinding, PromotionId, PromotionPolicyBinding,
    PromotionReviewEvidence, PublishedEvaluationEvidence, SelectionDecision, SelectionRecord,
    VariantDefinition, VariantId, identity::digest_parts,
};
use peritus_types::{EvidenceId, ProjectId, Sha256Digest};

/// One exact inert promotion action; it has no activation authority by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionProposal {
    id: PromotionId,
    project_id: ProjectId,
    campaign_id: EvolutionCampaignId,
    current: ProductionHarnessBinding,
    candidate: ProductionHarnessBinding,
    variant_id: VariantId,
    variant_digest: Sha256Digest,
    attribution_digest: Sha256Digest,
    evaluation_digest: Sha256Digest,
    review: Option<PromotionReviewEvidence>,
    policy_digest: Sha256Digest,
    selection_digest: Sha256Digest,
    rollback_target: ProductionHarnessBinding,
    evidence_bundle_artifact: Sha256Digest,
    digest: Sha256Digest,
}

impl PromotionProposal {
    /// Constructs a proposal from one exact selected and fully assessed variant.
    ///
    /// # Errors
    /// Rejects selection, evidence, policy, arm, or rollback drift.
    #[allow(
        clippy::too_many_arguments,
        reason = "the action digest keeps every authority fact explicit"
    )]
    pub fn new(
        project_id: ProjectId,
        campaign_id: EvolutionCampaignId,
        current: ProductionHarnessBinding,
        variant: &VariantDefinition,
        attribution: &AttributionRecord,
        evaluation: &PublishedEvaluationEvidence,
        review: Option<PromotionReviewEvidence>,
        policy: &PromotionPolicyBinding,
        selection: &SelectionRecord,
        evidence_bundle_artifact: Sha256Digest,
    ) -> Result<Self, EvolutionError> {
        if selection.decision() != &SelectionDecision::Selected(variant.id())
            || selection.policy_digest() != policy.policy().digest()
            || variant.baseline() != current
            || attribution.variant_id() != variant.id()
            || attribution.evaluation_digest() != evaluation.digest()
            || review.is_some_and(|value| {
                value.candidate_revision_digest()
                    != variant.candidate().harness_revision().digest().digest()
            })
        {
            return Err(binding());
        }
        let review_digest = review.map(PromotionReviewEvidence::digest);
        let digest = proposal_digest(
            project_id,
            campaign_id,
            current,
            variant.candidate(),
            variant.id(),
            variant.digest(),
            attribution.digest(),
            evaluation.digest(),
            review_digest,
            policy.digest(),
            selection.digest(),
            evidence_bundle_artifact,
        );
        Ok(Self {
            id: PromotionId::derive(b"peritus.f0.promotion-id.v1\0", digest),
            project_id,
            campaign_id,
            current,
            candidate: variant.candidate(),
            variant_id: variant.id(),
            variant_digest: variant.digest(),
            attribution_digest: attribution.digest(),
            evaluation_digest: evaluation.digest(),
            review,
            policy_digest: policy.digest(),
            selection_digest: selection.digest(),
            rollback_target: current,
            evidence_bundle_artifact,
            digest,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "every persisted promotion action fact stays explicit"
    )]
    pub(crate) fn from_exact_parts(
        project_id: ProjectId,
        campaign_id: EvolutionCampaignId,
        current: ProductionHarnessBinding,
        candidate: ProductionHarnessBinding,
        variant_id: VariantId,
        variant_digest: Sha256Digest,
        attribution_digest: Sha256Digest,
        evaluation_digest: Sha256Digest,
        review: Option<PromotionReviewEvidence>,
        policy_digest: Sha256Digest,
        selection_digest: Sha256Digest,
        rollback_target: ProductionHarnessBinding,
        evidence_bundle_artifact: Sha256Digest,
    ) -> Result<Self, EvolutionError> {
        if current == candidate || rollback_target != current {
            return Err(binding());
        }
        let digest = proposal_digest(
            project_id,
            campaign_id,
            current,
            candidate,
            variant_id,
            variant_digest,
            attribution_digest,
            evaluation_digest,
            review.map(PromotionReviewEvidence::digest),
            policy_digest,
            selection_digest,
            evidence_bundle_artifact,
        );
        Ok(Self {
            id: PromotionId::derive(b"peritus.f0.promotion-id.v1\0", digest),
            project_id,
            campaign_id,
            current,
            candidate,
            variant_id,
            variant_digest,
            attribution_digest,
            evaluation_digest,
            review,
            policy_digest,
            selection_digest,
            rollback_target,
            evidence_bundle_artifact,
            digest,
        })
    }

    /// Proposal identity.
    #[must_use]
    pub const fn id(&self) -> PromotionId {
        self.id
    }
    /// Project authority identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Owning campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvolutionCampaignId {
        self.campaign_id
    }
    /// Exact fenced current pointer.
    #[must_use]
    pub const fn current(&self) -> ProductionHarnessBinding {
        self.current
    }
    /// Exact candidate pointer.
    #[must_use]
    pub const fn candidate(&self) -> ProductionHarnessBinding {
        self.candidate
    }
    /// Selected variant.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Complete variant digest.
    #[must_use]
    pub const fn variant_digest(&self) -> Sha256Digest {
        self.variant_digest
    }
    /// Complete attribution digest.
    #[must_use]
    pub const fn attribution_digest(&self) -> Sha256Digest {
        self.attribution_digest
    }
    /// Published E3 bridge digest.
    #[must_use]
    pub const fn evaluation_digest(&self) -> Sha256Digest {
        self.evaluation_digest
    }
    /// Completed D2 review digest when supplied.
    #[must_use]
    pub const fn review_digest(&self) -> Option<Sha256Digest> {
        match self.review {
            Some(value) => Some(value.digest()),
            None => None,
        }
    }
    /// Complete terminal D2 bridge when review was required.
    #[must_use]
    pub const fn review(&self) -> Option<PromotionReviewEvidence> {
        self.review
    }
    /// Protected policy-binding digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Deterministic selection digest.
    #[must_use]
    pub const fn selection_digest(&self) -> Sha256Digest {
        self.selection_digest
    }
    /// Required rollback target.
    #[must_use]
    pub const fn rollback_target(&self) -> ProductionHarnessBinding {
        self.rollback_target
    }
    /// Finalized evidence-bundle artifact digest.
    #[must_use]
    pub const fn evidence_bundle_artifact(&self) -> Sha256Digest {
        self.evidence_bundle_artifact
    }
    /// Digest of the complete exact action.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// One acknowledged F0 decision publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CampaignPublication {
    artifact_digest: Sha256Digest,
    evidence_id: EvidenceId,
    evidence_digest: Sha256Digest,
    journal_position: u64,
}

impl CampaignPublication {
    /// Creates an exact artifact/evidence acknowledgement.
    ///
    /// # Errors
    /// Rejects the reserved zero journal position.
    pub const fn new(
        artifact_digest: Sha256Digest,
        evidence_id: EvidenceId,
        evidence_digest: Sha256Digest,
        journal_position: u64,
    ) -> Result<Self, EvolutionError> {
        if journal_position == 0 {
            return Err(EvolutionError::new(
                EvolutionErrorKind::InvalidInput,
                EvolutionOperation::TransitionCampaign,
                EvolutionRecovery::CorrectInput,
                "campaign publication journal position is zero",
            ));
        }
        Ok(Self { artifact_digest, evidence_id, evidence_digest, journal_position })
    }
    /// Finalized artifact digest.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Admitted evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Exact evidence record digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    /// Nonzero producing journal position used by evidence provenance.
    #[must_use]
    pub const fn journal_position(self) -> u64 {
        self.journal_position
    }
}

#[allow(clippy::too_many_arguments)]
fn proposal_digest(
    project: ProjectId,
    campaign: EvolutionCampaignId,
    current: ProductionHarnessBinding,
    candidate: ProductionHarnessBinding,
    variant_id: VariantId,
    variant_digest: Sha256Digest,
    attribution: Sha256Digest,
    evaluation: Sha256Digest,
    review: Option<Sha256Digest>,
    policy: Sha256Digest,
    selection: Sha256Digest,
    artifact: Sha256Digest,
) -> Sha256Digest {
    digest_parts(
        b"peritus.f0.promotion-proposal.v1\0",
        &[
            project.as_bytes(),
            campaign.as_bytes(),
            current.digest().as_bytes(),
            candidate.digest().as_bytes(),
            variant_id.as_bytes(),
            variant_digest.as_bytes(),
            attribution.as_bytes(),
            evaluation.as_bytes(),
            review.as_ref().map_or(&[][..], |value| value.as_bytes()),
            policy.as_bytes(),
            selection.as_bytes(),
            current.digest().as_bytes(),
            artifact.as_bytes(),
        ],
    )
}

const fn binding() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        "promotion proposal inputs do not name one selected exact candidate",
    )
}
