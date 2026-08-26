//! Complete pure evolution-campaign checkpoint.

use crate::{
    AttributionRecord, CampaignPublication, ChangeManifest, EvolutionCampaignId, EvolutionLimits,
    ProductionHarnessBinding, PromotionPolicyBinding, PromotionProposal, PublishedDebuggerEvidence,
    PublishedEvaluationEvidence, SelectionRecord, VariantAssessment, VariantDefinition, VariantId,
    identity::digest_parts,
};
use peritus_types::{EventId, ProjectId, Sha256Digest};

/// Durable campaign lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignPhase {
    /// Inputs may still be assembled.
    Draft,
    /// Baseline and policy are frozen.
    Frozen,
    /// Baseline evidence is being recorded.
    BaselineRunning,
    /// Published diagnosis is being admitted.
    Diagnosing,
    /// Change manifests and variants are being admitted.
    Proposing,
    /// Exact E3 evaluations are being admitted.
    VariantsRunning,
    /// Attribution and selection are being completed.
    Attributing,
    /// An exact selected proposal awaits atomic activation.
    PromotionReview,
    /// The candidate was atomically activated.
    Promoted,
    /// No candidate was eligible.
    Rejected,
    /// Campaign failed explicitly.
    Failed,
    /// Campaign was cancelled explicitly.
    Cancelled,
}

impl CampaignPhase {
    /// Returns whether no further semantic transition is legal.
    #[must_use]
    pub const fn terminal(self) -> bool {
        matches!(self, Self::Promoted | Self::Rejected | Self::Failed | Self::Cancelled)
    }

    pub(crate) const fn tag(self) -> u8 {
        self as u8
    }
}

/// Exact finalized baseline artifact/evidence pair.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BaselineEvidence {
    artifact_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
}

impl BaselineEvidence {
    /// Creates one exact baseline observation reference.
    #[must_use]
    pub const fn new(artifact_digest: Sha256Digest, evidence_digest: Sha256Digest) -> Self {
        Self { artifact_digest, evidence_digest }
    }
    /// Finalized baseline artifact.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Evidence record digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}

/// One variant-to-published-E3 evidence binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantEvaluation {
    variant_id: VariantId,
    evidence: PublishedEvaluationEvidence,
}

impl VariantEvaluation {
    pub(crate) const fn new(variant_id: VariantId, evidence: PublishedEvaluationEvidence) -> Self {
        Self { variant_id, evidence }
    }
    /// Variant identity.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Complete restart-consumable E3 bridge.
    #[must_use]
    pub const fn evidence(&self) -> &PublishedEvaluationEvidence {
        &self.evidence
    }
}

/// Truthful terminal campaign result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignTerminal {
    /// Exact candidate activation completed.
    Promoted {
        /// Activated promotion identity.
        promotion_id: crate::PromotionId,
        /// Exact atomic activation transaction digest.
        activation_digest: Sha256Digest,
    },
    /// Frozen selection found no eligible candidate.
    Rejected {
        /// Digest of the terminal deterministic selection.
        selection_digest: Sha256Digest,
    },
    /// Explicit typed failure digest.
    Failed {
        /// Stable digest of the failure reason evidence.
        reason_digest: Sha256Digest,
    },
    /// Explicit cancellation digest.
    Cancelled {
        /// Stable digest of the cancellation reason evidence.
        reason_digest: Sha256Digest,
    },
}

/// Complete authoritative pure campaign state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignState {
    pub(crate) campaign_id: EvolutionCampaignId,
    pub(crate) project_id: ProjectId,
    pub(crate) binding_digest: Sha256Digest,
    pub(crate) baseline: ProductionHarnessBinding,
    pub(crate) policy: PromotionPolicyBinding,
    pub(crate) limits: EvolutionLimits,
    pub(crate) sequence: u64,
    pub(crate) last_event: EventId,
    pub(crate) state_digest: Sha256Digest,
    pub(crate) phase: CampaignPhase,
    pub(crate) baseline_evidence: Vec<BaselineEvidence>,
    pub(crate) diagnoses: Vec<PublishedDebuggerEvidence>,
    pub(crate) manifests: Vec<ChangeManifest>,
    pub(crate) variants: Vec<VariantDefinition>,
    pub(crate) evaluations: Vec<VariantEvaluation>,
    pub(crate) attributions: Vec<AttributionRecord>,
    pub(crate) assessments: Vec<VariantAssessment>,
    pub(crate) selection: Option<SelectionRecord>,
    pub(crate) proposal: Option<PromotionProposal>,
    pub(crate) publication: Option<CampaignPublication>,
    pub(crate) terminal: Option<CampaignTerminal>,
}

impl CampaignState {
    /// Aggregate identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvolutionCampaignId {
        self.campaign_id
    }
    /// Project authority identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }
    /// Immutable campaign binding digest.
    #[must_use]
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }
    /// Exact frozen production baseline.
    #[must_use]
    pub const fn baseline(&self) -> ProductionHarnessBinding {
        self.baseline
    }
    /// Protected frozen policy binding.
    #[must_use]
    pub const fn policy(&self) -> &PromotionPolicyBinding {
        &self.policy
    }
    /// Caller-tightened bounds.
    #[must_use]
    pub const fn limits(&self) -> EvolutionLimits {
        self.limits
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
    /// Current lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> CampaignPhase {
        self.phase
    }
    /// Baseline artifact/evidence references.
    #[must_use]
    pub fn baseline_evidence(&self) -> &[BaselineEvidence] {
        &self.baseline_evidence
    }
    /// Published E2 bridges.
    #[must_use]
    pub fn diagnoses(&self) -> &[PublishedDebuggerEvidence] {
        &self.diagnoses
    }
    /// Canonical admitted manifests.
    #[must_use]
    pub fn manifests(&self) -> &[ChangeManifest] {
        &self.manifests
    }
    /// Canonical isolated variants.
    #[must_use]
    pub fn variants(&self) -> &[VariantDefinition] {
        &self.variants
    }
    /// Canonical exact E3 bridges.
    #[must_use]
    pub fn evaluations(&self) -> &[VariantEvaluation] {
        &self.evaluations
    }
    /// Deterministic attribution records.
    #[must_use]
    pub fn attributions(&self) -> &[AttributionRecord] {
        &self.attributions
    }
    /// Independent deny-wins assessments.
    #[must_use]
    pub fn assessments(&self) -> &[VariantAssessment] {
        &self.assessments
    }
    /// Frozen deterministic selection.
    #[must_use]
    pub const fn selection(&self) -> Option<&SelectionRecord> {
        self.selection.as_ref()
    }
    /// Exact inert promotion action.
    #[must_use]
    pub const fn proposal(&self) -> Option<&PromotionProposal> {
        self.proposal.as_ref()
    }
    /// Decision publication acknowledgement.
    #[must_use]
    pub const fn publication(&self) -> Option<CampaignPublication> {
        self.publication
    }
    /// Truthful terminal result.
    #[must_use]
    pub const fn terminal(&self) -> Option<CampaignTerminal> {
        self.terminal
    }

    pub(crate) fn refresh_digest(&mut self) {
        let mut semantic = Vec::new();
        semantic.extend_from_slice(self.binding_digest.as_bytes());
        semantic.extend_from_slice(&self.sequence.to_be_bytes());
        semantic.extend_from_slice(self.last_event.as_bytes());
        semantic.push(self.phase.tag());
        for value in &self.baseline_evidence {
            semantic.extend_from_slice(value.artifact_digest().as_bytes());
            semantic.extend_from_slice(value.evidence_digest().as_bytes());
        }
        append_digests(&mut semantic, self.diagnoses.iter().map(PublishedDebuggerEvidence::digest));
        append_digests(&mut semantic, self.manifests.iter().map(ChangeManifest::digest));
        append_digests(&mut semantic, self.variants.iter().map(VariantDefinition::digest));
        append_digests(
            &mut semantic,
            self.evaluations.iter().map(|value| value.evidence().digest()),
        );
        append_digests(&mut semantic, self.attributions.iter().map(AttributionRecord::digest));
        append_digests(&mut semantic, self.assessments.iter().map(VariantAssessment::digest));
        append_option(&mut semantic, self.selection.as_ref().map(SelectionRecord::digest));
        append_option(&mut semantic, self.proposal.as_ref().map(PromotionProposal::digest));
        if let Some(value) = self.publication {
            semantic.extend_from_slice(value.artifact_digest().as_bytes());
            semantic.extend_from_slice(value.evidence_id().as_bytes());
            semantic.extend_from_slice(value.evidence_digest().as_bytes());
            semantic.extend_from_slice(&value.journal_position().to_be_bytes());
        }
        append_terminal(&mut semantic, self.terminal);
        self.state_digest = digest_parts(b"peritus.f0.campaign-state.v1\0", &[&semantic]);
    }
}

fn append_digests(output: &mut Vec<u8>, values: impl Iterator<Item = Sha256Digest>) {
    for value in values {
        output.extend_from_slice(value.as_bytes());
    }
}

fn append_option(output: &mut Vec<u8>, value: Option<Sha256Digest>) {
    output.push(u8::from(value.is_some()));
    if let Some(value) = value {
        output.extend_from_slice(value.as_bytes());
    }
}

fn append_terminal(output: &mut Vec<u8>, terminal: Option<CampaignTerminal>) {
    match terminal {
        None => output.push(0),
        Some(CampaignTerminal::Promoted { promotion_id, activation_digest }) => {
            output.push(1);
            output.extend_from_slice(promotion_id.as_bytes());
            output.extend_from_slice(activation_digest.as_bytes());
        }
        Some(CampaignTerminal::Rejected { selection_digest }) => {
            output.push(2);
            output.extend_from_slice(selection_digest.as_bytes());
        }
        Some(CampaignTerminal::Failed { reason_digest }) => {
            output.push(3);
            output.extend_from_slice(reason_digest.as_bytes());
        }
        Some(CampaignTerminal::Cancelled { reason_digest }) => {
            output.push(4);
            output.extend_from_slice(reason_digest.as_bytes());
        }
    }
}
