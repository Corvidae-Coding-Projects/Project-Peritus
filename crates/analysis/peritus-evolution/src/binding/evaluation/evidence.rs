//! Published E3 evidence identity and binding checks.

use peritus_eval::{
    DatasetDigest, EvaluationArm, EvaluationCampaignId, EvaluationPhase, EvaluationPlanId,
    EvaluationReportId, EvaluationState, FrozenEvaluationProfile, HarnessArmBinding, PlanDigest,
    ProfileDigest, ValidatedEvaluationReport,
};
use peritus_types::{EvidenceId, Sha256Digest};

use super::EvaluationAnalysisSnapshot;
use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    ProductionHarnessBinding, identity::digest_parts,
};

/// Checked published E3 report retaining every value used by attribution and selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedEvaluationEvidence {
    campaign_id: EvaluationCampaignId,
    dataset_digest: DatasetDigest,
    profile_digest: ProfileDigest,
    plan_id: EvaluationPlanId,
    plan_digest: PlanDigest,
    baseline: HarnessArmBinding,
    candidate: HarnessArmBinding,
    report_id: EvaluationReportId,
    report_digest: Sha256Digest,
    report_artifact: Sha256Digest,
    evidence_id: EvidenceId,
    journal_position: u64,
    analysis: EvaluationAnalysisSnapshot,
    digest: Sha256Digest,
}

impl PublishedEvaluationEvidence {
    /// Captures one exact published E3 report for a declared baseline and candidate.
    ///
    /// # Errors
    /// Rejects non-published or drifted state, report/profile/plan disagreement, or arm bindings
    /// that differ from the F0 variant.
    pub fn capture(
        state: &EvaluationState,
        validated: &ValidatedEvaluationReport,
        profile: &FrozenEvaluationProfile,
        baseline: ProductionHarnessBinding,
        candidate: ProductionHarnessBinding,
    ) -> Result<Self, EvolutionError> {
        let report = validated.report();
        let record = state.report().ok_or_else(incomplete)?;
        let publication = state.publication().ok_or_else(incomplete)?;
        let plan = state.plan().ok_or_else(incomplete)?;
        let baseline_arm = profile.arm(EvaluationArm::Baseline);
        let candidate_arm = profile.arm(EvaluationArm::Candidate);
        if state.phase() != EvaluationPhase::Published
            || state.campaign_id() != report.campaign_id()
            || state.dataset_digest() != profile.dataset().digest()
            || state.dataset_digest() != report.dataset_digest()
            || state.profile_digest() != profile.digest()
            || state.profile_digest() != report.profile_digest()
            || plan.id() != report.plan_id()
            || plan.digest() != report.plan_digest()
            || state.analysis_digest() != Some(report.analysis().digest())
            || record.id() != validated.id()
            || record.payload_digest() != validated.digest()
            || record.size() != u64::try_from(validated.bytes().len()).unwrap_or(u64::MAX)
            || publication.report_id() != validated.id()
            || !arm_matches(baseline_arm, baseline)
            || !arm_matches(candidate_arm, candidate)
            || *state.revision() != baseline.revision()
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::BindingDrift,
                EvolutionOperation::BindEvaluation,
                EvolutionRecovery::CorrectInput,
                "evaluation state, report, profile, plan, or harness arm differs",
            ));
        }
        let report_artifact = record.artifact().sha256();
        let analysis = EvaluationAnalysisSnapshot::capture(report.analysis());
        let digest = digest_parts(
            b"peritus.f0.published-evaluation-evidence.v1\0",
            &[
                state.campaign_id().as_bytes(),
                state.dataset_digest().as_bytes(),
                state.profile_digest().as_bytes(),
                plan.id().as_bytes(),
                plan.digest().as_bytes(),
                baseline_arm.digest().as_bytes(),
                candidate_arm.digest().as_bytes(),
                validated.id().as_bytes(),
                validated.digest().as_bytes(),
                report_artifact.as_bytes(),
                publication.evidence_id().as_bytes(),
                &publication.report_commit_position().to_be_bytes(),
                report.analysis().digest().as_bytes(),
                analysis.digest().as_bytes(),
            ],
        );
        Ok(Self {
            campaign_id: state.campaign_id(),
            dataset_digest: state.dataset_digest(),
            profile_digest: state.profile_digest(),
            plan_id: plan.id(),
            plan_digest: plan.digest(),
            baseline: baseline_arm,
            candidate: candidate_arm,
            report_id: validated.id(),
            report_digest: validated.digest(),
            report_artifact,
            evidence_id: publication.evidence_id(),
            journal_position: publication.report_commit_position(),
            analysis,
            digest,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "every persisted E3 bridge fact stays explicit")]
    pub(crate) fn from_exact_parts(
        campaign_id: EvaluationCampaignId,
        dataset_digest: DatasetDigest,
        profile_digest: ProfileDigest,
        plan_id: EvaluationPlanId,
        plan_digest: PlanDigest,
        baseline: HarnessArmBinding,
        candidate: HarnessArmBinding,
        report_id: EvaluationReportId,
        report_digest: Sha256Digest,
        report_artifact: Sha256Digest,
        evidence_id: EvidenceId,
        journal_position: u64,
        analysis: EvaluationAnalysisSnapshot,
    ) -> Result<Self, EvolutionError> {
        if journal_position == 0 || baseline == candidate {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Corruption,
                EvolutionOperation::BindEvaluation,
                EvolutionRecovery::Quarantine,
                "persisted evaluation evidence has an invalid journal position or equal arms",
            ));
        }
        let digest = digest_parts(
            b"peritus.f0.published-evaluation-evidence.v1\0",
            &[
                campaign_id.as_bytes(),
                dataset_digest.as_bytes(),
                profile_digest.as_bytes(),
                plan_id.as_bytes(),
                plan_digest.as_bytes(),
                baseline.digest().as_bytes(),
                candidate.digest().as_bytes(),
                report_id.as_bytes(),
                report_digest.as_bytes(),
                report_artifact.as_bytes(),
                evidence_id.as_bytes(),
                &journal_position.to_be_bytes(),
                analysis.source_digest().as_bytes(),
                analysis.digest().as_bytes(),
            ],
        );
        Ok(Self {
            campaign_id,
            dataset_digest,
            profile_digest,
            plan_id,
            plan_digest,
            baseline,
            candidate,
            report_id,
            report_digest,
            report_artifact,
            evidence_id,
            journal_position,
            analysis,
            digest,
        })
    }

    /// Returns the E3 campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Returns the frozen dataset digest.
    #[must_use]
    pub const fn dataset_digest(&self) -> DatasetDigest {
        self.dataset_digest
    }
    /// Returns the frozen evaluation-profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> ProfileDigest {
        self.profile_digest
    }
    /// Returns the deterministic plan identity.
    #[must_use]
    pub const fn plan_id(&self) -> EvaluationPlanId {
        self.plan_id
    }
    /// Returns the deterministic plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> PlanDigest {
        self.plan_digest
    }
    /// Returns the exact baseline arm.
    #[must_use]
    pub const fn baseline(&self) -> HarnessArmBinding {
        self.baseline
    }
    /// Returns the exact candidate arm.
    #[must_use]
    pub const fn candidate(&self) -> HarnessArmBinding {
        self.candidate
    }
    /// Returns the validated report identity.
    #[must_use]
    pub const fn report_id(&self) -> EvaluationReportId {
        self.report_id
    }
    /// Returns the canonical report payload digest.
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest {
        self.report_digest
    }
    /// Returns the finalized report artifact digest.
    #[must_use]
    pub const fn report_artifact(&self) -> Sha256Digest {
        self.report_artifact
    }
    /// Returns the admitted C0 evidence identity.
    #[must_use]
    pub const fn evidence_id(&self) -> EvidenceId {
        self.evidence_id
    }
    /// Returns the journal position cited by evidence provenance.
    #[must_use]
    pub const fn journal_position(&self) -> u64 {
        self.journal_position
    }
    /// Borrows the complete E3 analysis used by F0.
    #[must_use]
    pub const fn analysis(&self) -> &EvaluationAnalysisSnapshot {
        &self.analysis
    }
    /// Returns the digest of every retained evaluation fact.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn arm_matches(arm: HarnessArmBinding, binding: ProductionHarnessBinding) -> bool {
    arm.revision() == binding.revision()
        && arm.harness_revision() == binding.harness_revision()
        && arm.receipt_digest() == binding.materialization_receipt_digest()
}

const fn incomplete() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::IncompleteEvidence,
        EvolutionOperation::BindEvaluation,
        EvolutionRecovery::ObtainEvidence,
        "evaluation report is not published",
    )
}
