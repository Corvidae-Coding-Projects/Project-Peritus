//! Closed CAS-fenced evolution-campaign commands.

use crate::{
    AttributionRecord, CampaignPublication, ChangeManifest, EvolutionCampaignId, EvolutionError,
    EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
    ProductionHarnessBinding, PromotionPolicyBinding, PromotionProposal, PublishedDebuggerEvidence,
    PublishedEvaluationEvidence, SelectionRecord, VariantAssessment, VariantDefinition, VariantId,
    identity::digest_parts,
};
use peritus_types::{CommandId, EventId, ProjectId, Sha256Digest};

/// Closed campaign command semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignCommandKind {
    /// Creates a draft campaign from the exact current pointer and policy.
    CreateCampaign {
        /// Project authority.
        project_id: ProjectId,
        /// Exact production baseline.
        baseline: ProductionHarnessBinding,
        /// Protected frozen policy.
        policy: PromotionPolicyBinding,
        /// Caller-tightened bounds.
        limits: EvolutionLimits,
    },
    /// Freezes all immutable campaign inputs.
    FreezeCampaign,
    /// Records one immutable baseline evidence artifact and evidence record.
    RecordBaselineEvidence {
        /// Finalized immutable baseline artifact digest.
        artifact_digest: Sha256Digest,
        /// Exact baseline evidence-record digest.
        evidence_digest: Sha256Digest,
    },
    /// Admits one published E2 evidence bridge.
    SubmitDiagnosis(PublishedDebuggerEvidence),
    /// Admits one exact change manifest.
    AdmitChangeManifest(ChangeManifest),
    /// Admits one isolated E1 variant.
    AdmitVariant(VariantDefinition),
    /// Admits one exact E3 report for a variant.
    AdmitEvaluation {
        /// Variant evaluated by E3.
        variant_id: VariantId,
        /// Complete published evaluation evidence.
        evidence: PublishedEvaluationEvidence,
    },
    /// Records deterministic attribution and the independent policy assessment.
    CompleteAttribution {
        /// Deterministic prediction-to-observation attribution.
        attribution: AttributionRecord,
        /// Independent frozen-policy assessment.
        assessment: VariantAssessment,
    },
    /// Records the deterministic campaign-wide selection.
    RecordSelection(SelectionRecord),
    /// Freezes the exact inert promotion action.
    RequestPromotion(PromotionProposal),
    /// Atomically accepted external activation digest.
    ActivatePromotion {
        /// Digest binding the exact atomic activation transaction.
        activation_digest: Sha256Digest,
    },
    /// Acknowledges one finalized decision publication.
    RecordPublication(CampaignPublication),
    /// Cancels a nonterminal campaign.
    CancelCampaign {
        /// Stable digest of the cancellation reason evidence.
        reason_digest: Sha256Digest,
    },
    /// Fails a nonterminal campaign.
    FailCampaign {
        /// Stable digest of the failure reason evidence.
        reason_digest: Sha256Digest,
    },
}

/// One exact campaign command with optimistic state fences.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCommand {
    command_id: CommandId,
    event_id: EventId,
    campaign_id: EvolutionCampaignId,
    expected_sequence: u64,
    expected_head: Option<EventId>,
    prior_state_digest: Sha256Digest,
    policy_digest: Sha256Digest,
    kind: CampaignCommandKind,
    digest: Sha256Digest,
}

impl CampaignCommand {
    /// Constructs a fully fenced command.
    ///
    /// # Errors
    /// Rejects inconsistent genesis/head fencing.
    #[allow(
        clippy::too_many_arguments,
        reason = "all optimistic and immutable fences are explicit"
    )]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        campaign_id: EvolutionCampaignId,
        expected_sequence: u64,
        expected_head: Option<EventId>,
        prior_state_digest: Sha256Digest,
        policy_digest: Sha256Digest,
        kind: CampaignCommandKind,
    ) -> Result<Self, EvolutionError> {
        if (expected_sequence == 0) != expected_head.is_none()
            || matches!(&kind, CampaignCommandKind::CreateCampaign { policy, .. }
                if policy.policy().digest() != policy_digest)
        {
            return Err(invalid());
        }
        let digest = command_digest(
            command_id,
            event_id,
            campaign_id,
            expected_sequence,
            expected_head,
            prior_state_digest,
            policy_digest,
            &kind,
        );
        Ok(Self {
            command_id,
            event_id,
            campaign_id,
            expected_sequence,
            expected_head,
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
    /// Reserved semantic event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Aggregate identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvolutionCampaignId {
        self.campaign_id
    }
    /// Expected applied sequence.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Expected aggregate head.
    #[must_use]
    pub const fn expected_head(&self) -> Option<EventId> {
        self.expected_head
    }
    /// Expected complete prior state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Frozen typed-policy digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Exact command semantics.
    #[must_use]
    pub const fn kind(&self) -> &CampaignCommandKind {
        &self.kind
    }
    /// Digest of every command identity and semantic field.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[allow(clippy::too_many_arguments)]
fn command_digest(
    command: CommandId,
    event: EventId,
    campaign: EvolutionCampaignId,
    sequence: u64,
    head: Option<EventId>,
    prior: Sha256Digest,
    policy: Sha256Digest,
    kind: &CampaignCommandKind,
) -> Sha256Digest {
    let sequence = sequence.to_be_bytes();
    let semantic = semantic_digest(kind);
    digest_parts(
        b"peritus.f0.campaign-command.v1\0",
        &[
            command.as_bytes(),
            event.as_bytes(),
            campaign.as_bytes(),
            &sequence,
            head.as_ref().map_or(&[][..], |value| value.as_bytes()),
            prior.as_bytes(),
            policy.as_bytes(),
            semantic.as_bytes(),
        ],
    )
}

pub(crate) fn semantic_digest(kind: &CampaignCommandKind) -> Sha256Digest {
    let mut bytes = Vec::new();
    match kind {
        CampaignCommandKind::CreateCampaign { project_id, baseline, policy, limits } => {
            bytes.push(1);
            bytes.extend_from_slice(project_id.as_bytes());
            bytes.extend_from_slice(baseline.digest().as_bytes());
            bytes.extend_from_slice(policy.digest().as_bytes());
            bytes.extend_from_slice(limits.digest().as_bytes());
        }
        CampaignCommandKind::FreezeCampaign => bytes.push(2),
        CampaignCommandKind::RecordBaselineEvidence { artifact_digest, evidence_digest } => {
            bytes.push(3);
            bytes.extend_from_slice(artifact_digest.as_bytes());
            bytes.extend_from_slice(evidence_digest.as_bytes());
        }
        CampaignCommandKind::SubmitDiagnosis(value) => append(&mut bytes, 4, value.digest()),
        CampaignCommandKind::AdmitChangeManifest(value) => append(&mut bytes, 5, value.digest()),
        CampaignCommandKind::AdmitVariant(value) => append(&mut bytes, 6, value.digest()),
        CampaignCommandKind::AdmitEvaluation { variant_id, evidence } => {
            bytes.push(7);
            bytes.extend_from_slice(variant_id.as_bytes());
            bytes.extend_from_slice(evidence.digest().as_bytes());
        }
        CampaignCommandKind::CompleteAttribution { attribution, assessment } => {
            bytes.push(8);
            bytes.extend_from_slice(attribution.digest().as_bytes());
            bytes.extend_from_slice(assessment.digest().as_bytes());
        }
        CampaignCommandKind::RecordSelection(value) => append(&mut bytes, 9, value.digest()),
        CampaignCommandKind::RequestPromotion(value) => append(&mut bytes, 10, value.digest()),
        CampaignCommandKind::ActivatePromotion { activation_digest } => {
            append(&mut bytes, 11, *activation_digest);
        }
        CampaignCommandKind::RecordPublication(value) => {
            bytes.push(12);
            bytes.extend_from_slice(value.artifact_digest().as_bytes());
            bytes.extend_from_slice(value.evidence_id().as_bytes());
            bytes.extend_from_slice(value.evidence_digest().as_bytes());
            bytes.extend_from_slice(&value.journal_position().to_be_bytes());
        }
        CampaignCommandKind::CancelCampaign { reason_digest } => {
            append(&mut bytes, 13, *reason_digest);
        }
        CampaignCommandKind::FailCampaign { reason_digest } => {
            append(&mut bytes, 14, *reason_digest);
        }
    }
    digest_parts(b"peritus.f0.campaign-command-kind.v1\0", &[&bytes])
}

fn append(bytes: &mut Vec<u8>, tag: u8, digest: Sha256Digest) {
    bytes.push(tag);
    bytes.extend_from_slice(digest.as_bytes());
}

const fn invalid() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::InvalidInput,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        "campaign command fence or immutable policy digest is inconsistent",
    )
}
