//! Durable per-variant evaluation generations and exact supersession lineage.

use peritus_eval::{EvaluationCampaignId, EvaluationReportId};
use peritus_types::{EvidenceId, Sha256Digest};

use crate::{
    AttributionRecord, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    PublishedEvaluationEvidence, ReconciledEvaluationSuccessor, VariantAssessment, VariantId,
    identity::digest_parts,
};

/// Exact durable proof that one active evaluation was replaced by its reconciled successor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationSupersession {
    variant_id: VariantId,
    predecessor_evaluation_digest: Sha256Digest,
    predecessor_campaign_id: EvaluationCampaignId,
    predecessor_report_id: EvaluationReportId,
    predecessor_report_digest: Sha256Digest,
    predecessor_report_artifact: Sha256Digest,
    predecessor_evidence_id: EvidenceId,
    reconciliation_digest: Sha256Digest,
    successor: PublishedEvaluationEvidence,
    digest: Sha256Digest,
}

impl EvaluationSupersession {
    /// Captures an explicit replacement after checking the proof against the active predecessor.
    ///
    /// # Errors
    /// Rejects a witness for another predecessor, variant lineage, or logical evaluation.
    pub fn new(
        variant_id: VariantId,
        predecessor: &PublishedEvaluationEvidence,
        reconciliation: &ReconciledEvaluationSuccessor,
    ) -> Result<Self, EvolutionError> {
        Self::from_exact_parts(
            variant_id,
            predecessor.digest(),
            reconciliation.predecessor_campaign_id(),
            reconciliation.predecessor_report_id(),
            reconciliation.predecessor_report_digest(),
            reconciliation.predecessor_report_artifact(),
            reconciliation.predecessor_evidence_id(),
            reconciliation.digest(),
            reconciliation.successor().clone(),
        )
        .and_then(|value| {
            if value.matches_predecessor(predecessor) { Ok(value) } else { Err(binding()) }
        })
    }

    #[allow(clippy::too_many_arguments, reason = "the complete predecessor lineage stays explicit")]
    pub(crate) fn from_exact_parts(
        variant_id: VariantId,
        predecessor_evaluation_digest: Sha256Digest,
        predecessor_campaign_id: EvaluationCampaignId,
        predecessor_report_id: EvaluationReportId,
        predecessor_report_digest: Sha256Digest,
        predecessor_report_artifact: Sha256Digest,
        predecessor_evidence_id: EvidenceId,
        reconciliation_digest: Sha256Digest,
        successor: PublishedEvaluationEvidence,
    ) -> Result<Self, EvolutionError> {
        if successor.campaign_id() == predecessor_campaign_id
            || successor.report_id() == predecessor_report_id
            || successor.digest() == predecessor_evaluation_digest
        {
            return Err(binding());
        }
        let digest = supersession_digest(
            variant_id,
            predecessor_evaluation_digest,
            predecessor_campaign_id,
            predecessor_report_id,
            predecessor_report_digest,
            predecessor_report_artifact,
            predecessor_evidence_id,
            reconciliation_digest,
            successor.digest(),
        );
        Ok(Self {
            variant_id,
            predecessor_evaluation_digest,
            predecessor_campaign_id,
            predecessor_report_id,
            predecessor_report_digest,
            predecessor_report_artifact,
            predecessor_evidence_id,
            reconciliation_digest,
            successor,
            digest,
        })
    }

    /// Variant whose active evidence is replaced.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Complete digest of the exact active predecessor evidence.
    #[must_use]
    pub const fn predecessor_evaluation_digest(&self) -> Sha256Digest {
        self.predecessor_evaluation_digest
    }
    /// Immutable predecessor E3 campaign.
    #[must_use]
    pub const fn predecessor_campaign_id(&self) -> EvaluationCampaignId {
        self.predecessor_campaign_id
    }
    /// Report explicitly named by the successor report lineage.
    #[must_use]
    pub const fn predecessor_report_id(&self) -> EvaluationReportId {
        self.predecessor_report_id
    }
    /// Canonical predecessor report payload digest.
    #[must_use]
    pub const fn predecessor_report_digest(&self) -> Sha256Digest {
        self.predecessor_report_digest
    }
    /// Finalized predecessor report artifact.
    #[must_use]
    pub const fn predecessor_report_artifact(&self) -> Sha256Digest {
        self.predecessor_report_artifact
    }
    /// Admitted predecessor evidence identity.
    #[must_use]
    pub const fn predecessor_evidence_id(&self) -> EvidenceId {
        self.predecessor_evidence_id
    }
    /// Digest of both publication ownership chains and strict report lineage.
    #[must_use]
    pub const fn reconciliation_digest(&self) -> Sha256Digest {
        self.reconciliation_digest
    }
    /// Complete successor evaluation evidence.
    #[must_use]
    pub const fn successor(&self) -> &PublishedEvaluationEvidence {
        &self.successor
    }
    /// Digest of the exact replacement record.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(crate) fn matches_predecessor(&self, predecessor: &PublishedEvaluationEvidence) -> bool {
        self.predecessor_evaluation_digest == predecessor.digest()
            && self.predecessor_campaign_id == predecessor.campaign_id()
            && self.predecessor_report_id == predecessor.report_id()
            && self.predecessor_report_digest == predecessor.report_digest()
            && self.predecessor_report_artifact == predecessor.report_artifact()
            && self.predecessor_evidence_id == predecessor.evidence_id()
            && self.successor.baseline() == predecessor.baseline()
            && self.successor.candidate() == predecessor.candidate()
            && self.successor.dataset_digest() == predecessor.dataset_digest()
            && self.successor.profile_digest() == predecessor.profile_digest()
    }
}

/// One immutable evaluation generation and its generation-local eligibility work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationGeneration {
    variant_id: VariantId,
    generation: u64,
    evidence: PublishedEvaluationEvidence,
    supersession: Option<EvaluationSupersession>,
    attribution: Option<AttributionRecord>,
    assessment: Option<VariantAssessment>,
    digest: Sha256Digest,
}

impl EvaluationGeneration {
    pub(crate) fn initial(
        variant_id: VariantId,
        evidence: PublishedEvaluationEvidence,
        attribution: Option<AttributionRecord>,
        assessment: Option<VariantAssessment>,
    ) -> Result<Self, EvolutionError> {
        Self::from_exact_parts(variant_id, 1, evidence, None, attribution, assessment)
    }

    pub(crate) fn successor(
        generation: u64,
        supersession: EvaluationSupersession,
    ) -> Result<Self, EvolutionError> {
        Self::from_exact_parts(
            supersession.variant_id(),
            generation,
            supersession.successor().clone(),
            Some(supersession),
            None,
            None,
        )
    }

    pub(crate) fn from_exact_parts(
        variant_id: VariantId,
        generation: u64,
        evidence: PublishedEvaluationEvidence,
        supersession: Option<EvaluationSupersession>,
        attribution: Option<AttributionRecord>,
        assessment: Option<VariantAssessment>,
    ) -> Result<Self, EvolutionError> {
        if generation == 0
            || (generation == 1) != supersession.is_none()
            || supersession.as_ref().is_some_and(|value| {
                value.variant_id() != variant_id || value.successor() != &evidence
            })
            || attribution.as_ref().is_some_and(|value| {
                value.variant_id() != variant_id || value.evaluation_digest() != evidence.digest()
            })
            || assessment.as_ref().is_some_and(|value| {
                value.variant_id() != variant_id
                    || value.evidence_digest() != evidence.digest()
                    || attribution
                        .as_ref()
                        .is_none_or(|record| value.attribution_id() != record.id())
            })
        {
            return Err(binding());
        }
        let digest = generation_digest(
            variant_id,
            generation,
            evidence.digest(),
            supersession.as_ref().map(EvaluationSupersession::digest),
            attribution.as_ref().map(AttributionRecord::digest),
            assessment.as_ref().map(VariantAssessment::digest),
        );
        Ok(Self {
            variant_id,
            generation,
            evidence,
            supersession,
            attribution,
            assessment,
            digest,
        })
    }

    pub(crate) fn attach(
        &mut self,
        attribution: AttributionRecord,
        assessment: VariantAssessment,
    ) -> Result<(), EvolutionError> {
        if self.attribution.is_some() || self.assessment.is_some() {
            return Err(binding());
        }
        *self = Self::from_exact_parts(
            self.variant_id,
            self.generation,
            self.evidence.clone(),
            self.supersession.clone(),
            Some(attribution),
            Some(assessment),
        )?;
        Ok(())
    }

    /// Owning variant.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Positive per-variant evidence generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Complete evidence snapshot for this generation.
    #[must_use]
    pub const fn evidence(&self) -> &PublishedEvaluationEvidence {
        &self.evidence
    }
    /// Explicit predecessor lineage for generations after the first.
    #[must_use]
    pub const fn supersession(&self) -> Option<&EvaluationSupersession> {
        self.supersession.as_ref()
    }
    /// Attribution snapshot derived from this generation, when completed.
    #[must_use]
    pub const fn attribution(&self) -> Option<&AttributionRecord> {
        self.attribution.as_ref()
    }
    /// Independent eligibility snapshot derived from this generation, when completed.
    #[must_use]
    pub const fn assessment(&self) -> Option<&VariantAssessment> {
        self.assessment.as_ref()
    }
    /// Digest of this complete immutable generation snapshot.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Current per-variant evidence progress projected from durable campaign truth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VariantEvidenceStage {
    /// The immutable candidate exists but has no published evaluation.
    Proposed,
    /// Active published evaluation evidence exists.
    Evaluated,
    /// Active evaluation has deterministic attribution.
    Attributed,
    /// Active evaluation has an independent policy assessment.
    Assessed,
}

/// Current generation and dependent snapshots for one admitted variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VariantEvidenceLifecycle {
    variant_id: VariantId,
    generation: u64,
    stage: VariantEvidenceStage,
    evaluation_digest: Option<Sha256Digest>,
    attribution_digest: Option<Sha256Digest>,
    assessment_digest: Option<Sha256Digest>,
}

impl VariantEvidenceLifecycle {
    pub(crate) const fn new(
        variant_id: VariantId,
        generation: u64,
        stage: VariantEvidenceStage,
        evaluation_digest: Option<Sha256Digest>,
        attribution_digest: Option<Sha256Digest>,
        assessment_digest: Option<Sha256Digest>,
    ) -> Self {
        Self {
            variant_id,
            generation,
            stage,
            evaluation_digest,
            attribution_digest,
            assessment_digest,
        }
    }
    /// Admitted variant.
    #[must_use]
    pub const fn variant_id(self) -> VariantId {
        self.variant_id
    }
    /// Active positive evaluation generation, or zero before evaluation.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
    /// Current generation-local progress.
    #[must_use]
    pub const fn stage(self) -> VariantEvidenceStage {
        self.stage
    }
    /// Active evaluation evidence digest.
    #[must_use]
    pub const fn evaluation_digest(self) -> Option<Sha256Digest> {
        self.evaluation_digest
    }
    /// Attribution digest bound to the active evidence.
    #[must_use]
    pub const fn attribution_digest(self) -> Option<Sha256Digest> {
        self.attribution_digest
    }
    /// Independent assessment digest bound to the active evidence.
    #[must_use]
    pub const fn assessment_digest(self) -> Option<Sha256Digest> {
        self.assessment_digest
    }
}

#[allow(clippy::too_many_arguments)]
fn supersession_digest(
    variant_id: VariantId,
    predecessor_evaluation_digest: Sha256Digest,
    predecessor_campaign_id: EvaluationCampaignId,
    predecessor_report_id: EvaluationReportId,
    predecessor_report_digest: Sha256Digest,
    predecessor_report_artifact: Sha256Digest,
    predecessor_evidence_id: EvidenceId,
    reconciliation_digest: Sha256Digest,
    successor_digest: Sha256Digest,
) -> Sha256Digest {
    digest_parts(
        b"peritus.f0.evaluation-supersession.v1\0",
        &[
            variant_id.as_bytes(),
            predecessor_evaluation_digest.as_bytes(),
            predecessor_campaign_id.as_bytes(),
            predecessor_report_id.as_bytes(),
            predecessor_report_digest.as_bytes(),
            predecessor_report_artifact.as_bytes(),
            predecessor_evidence_id.as_bytes(),
            reconciliation_digest.as_bytes(),
            successor_digest.as_bytes(),
        ],
    )
}

fn generation_digest(
    variant_id: VariantId,
    generation: u64,
    evidence: Sha256Digest,
    supersession: Option<Sha256Digest>,
    attribution: Option<Sha256Digest>,
    assessment: Option<Sha256Digest>,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(16 + 8 + 32 * 4 + 3);
    bytes.extend_from_slice(variant_id.as_bytes());
    bytes.extend_from_slice(&generation.to_be_bytes());
    bytes.extend_from_slice(evidence.as_bytes());
    append_option(&mut bytes, supersession);
    append_option(&mut bytes, attribution);
    append_option(&mut bytes, assessment);
    digest_parts(b"peritus.f0.evaluation-generation.v1\0", &[&bytes])
}

fn append_option(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(value.as_bytes());
    }
}

const fn binding() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        "evaluation successor does not continue the exact active variant evidence",
    )
}
