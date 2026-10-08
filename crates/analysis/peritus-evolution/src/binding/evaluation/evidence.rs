//! Published E3 evidence identity and binding checks.

use peritus_eval::{
    DatasetDigest, EvaluationArm, EvaluationCampaignId, EvaluationOperationReceipt,
    EvaluationPhase, EvaluationPlanId, EvaluationReportId, EvaluationState,
    FrozenEvaluationProfile, HarnessArmBinding, PlanDigest, ProfileDigest,
    PublicationOwnershipReceipt, ValidatedEvaluationReport,
};
use peritus_journal::OutboxId;
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

/// Explicit F0 witness that a repaired E3 publication succeeds an immutable published campaign.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciledEvaluationSuccessor {
    predecessor_campaign_id: EvaluationCampaignId,
    predecessor_report_id: EvaluationReportId,
    predecessor_report_digest: Sha256Digest,
    predecessor_report_artifact: Sha256Digest,
    predecessor_evidence_id: EvidenceId,
    predecessor_report_operation: EvaluationOperationReceipt,
    predecessor_publication_operation: EvaluationOperationReceipt,
    predecessor_outbox_id: OutboxId,
    predecessor_fence: u64,
    successor_report_operation: EvaluationOperationReceipt,
    successor_publication_operation: EvaluationOperationReceipt,
    successor_outbox_id: OutboxId,
    successor_fence: u64,
    successor: PublishedEvaluationEvidence,
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
            || state
                .analysis_artifact()
                .is_none_or(|artifact| artifact.sha256() != report.analysis().digest().digest())
            || state.analysis_artifact_bytes().is_none_or(|size| size == 0)
            || record.id() != validated.id()
            || record.payload_digest() != validated.digest()
            || record.artifact().sha256() != peritus_codec::sha256(validated.bytes())
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

impl ReconciledEvaluationSuccessor {
    /// Captures a strict successor while retaining both accepted publication ownership chains.
    ///
    /// The predecessor remains immutable. The successor must use another campaign, retain the
    /// same revision, dataset, profile, and harness arms, and its canonical report must explicitly
    /// supersede the predecessor report.
    ///
    /// # Errors
    /// Rejects an unowned publication, unrelated logical work, absent report lineage, or any
    /// successor that fails the ordinary exact F0 evaluation checks.
    #[allow(
        clippy::too_many_arguments,
        reason = "both immutable publications and the complete shared logical work stay explicit"
    )]
    pub fn capture(
        predecessor_state: &EvaluationState,
        predecessor_report: &ValidatedEvaluationReport,
        predecessor_ownership: PublicationOwnershipReceipt,
        successor_state: &EvaluationState,
        successor_report: &ValidatedEvaluationReport,
        successor_ownership: PublicationOwnershipReceipt,
        profile: &FrozenEvaluationProfile,
        baseline: ProductionHarnessBinding,
        candidate: ProductionHarnessBinding,
    ) -> Result<Self, EvolutionError> {
        let successor = PublishedEvaluationEvidence::capture(
            successor_state,
            successor_report,
            profile,
            baseline,
            candidate,
        )?;
        let predecessor_record = predecessor_state.report().ok_or_else(incomplete)?;
        let predecessor_publication = predecessor_state.publication().ok_or_else(incomplete)?;
        let predecessor_size = u64::try_from(predecessor_report.bytes().len()).unwrap_or(u64::MAX);
        if predecessor_state.phase() != EvaluationPhase::Published
            || predecessor_state.campaign_id() == successor_state.campaign_id()
            || predecessor_state.revision() != successor_state.revision()
            || predecessor_state.dataset_digest() != successor_state.dataset_digest()
            || predecessor_state.profile_digest() != successor_state.profile_digest()
            || predecessor_state.profile_digest() != profile.digest()
            || !predecessor_ownership.matches_state(predecessor_state)
            || !successor_ownership.matches_state(successor_state)
            || predecessor_record.id() != predecessor_report.id()
            || predecessor_record.payload_digest() != predecessor_report.digest()
            || predecessor_record.artifact().sha256()
                != peritus_codec::sha256(predecessor_report.bytes())
            || predecessor_record.size() != predecessor_size
            || predecessor_publication.report_id() != predecessor_report.id()
            || successor_report.report().supersedes() != Some(predecessor_report.id())
            || !arm_matches(profile.arm(EvaluationArm::Baseline), baseline)
            || !arm_matches(profile.arm(EvaluationArm::Candidate), candidate)
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::BindingDrift,
                EvolutionOperation::BindEvaluation,
                EvolutionRecovery::CorrectInput,
                "reconciled evaluation successor is not the same owned logical work",
            ));
        }
        let predecessor_report_operation = predecessor_ownership.report_operation();
        let predecessor_publication_operation = predecessor_ownership.publication_operation();
        let successor_report_operation = successor_ownership.report_operation();
        let successor_publication_operation = successor_ownership.publication_operation();
        let predecessor_outbox_id = predecessor_ownership.outbox_id();
        let successor_outbox_id = successor_ownership.outbox_id();
        let predecessor_fence = predecessor_ownership.fence();
        let successor_fence = successor_ownership.fence();
        let predecessor_report_receipt = operation_receipt_digest(predecessor_report_operation);
        let predecessor_publication_receipt =
            operation_receipt_digest(predecessor_publication_operation);
        let successor_report_receipt = operation_receipt_digest(successor_report_operation);
        let successor_publication_receipt =
            operation_receipt_digest(successor_publication_operation);
        let predecessor_fence_bytes = predecessor_fence.to_be_bytes();
        let successor_fence_bytes = successor_fence.to_be_bytes();
        let digest = digest_parts(
            b"peritus.f0.reconciled-evaluation-successor.v1\0",
            &[
                predecessor_state.campaign_id().as_bytes(),
                predecessor_report.id().as_bytes(),
                predecessor_report.digest().as_bytes(),
                predecessor_record.artifact().as_bytes(),
                predecessor_publication.evidence_id().as_bytes(),
                predecessor_report_receipt.as_bytes(),
                predecessor_publication_receipt.as_bytes(),
                predecessor_outbox_id.as_bytes(),
                &predecessor_fence_bytes,
                successor_report_receipt.as_bytes(),
                successor_publication_receipt.as_bytes(),
                successor_outbox_id.as_bytes(),
                &successor_fence_bytes,
                successor.digest().as_bytes(),
            ],
        );
        Ok(Self {
            predecessor_campaign_id: predecessor_state.campaign_id(),
            predecessor_report_id: predecessor_report.id(),
            predecessor_report_digest: predecessor_report.digest(),
            predecessor_report_artifact: predecessor_record.artifact().sha256(),
            predecessor_evidence_id: predecessor_publication.evidence_id(),
            predecessor_report_operation,
            predecessor_publication_operation,
            predecessor_outbox_id,
            predecessor_fence,
            successor_report_operation,
            successor_publication_operation,
            successor_outbox_id,
            successor_fence,
            successor,
            digest,
        })
    }

    /// Original terminal E3 campaign retained by the repair.
    #[must_use]
    pub const fn predecessor_campaign_id(&self) -> EvaluationCampaignId {
        self.predecessor_campaign_id
    }
    /// Original report explicitly named by the successor report.
    #[must_use]
    pub const fn predecessor_report_id(&self) -> EvaluationReportId {
        self.predecessor_report_id
    }
    /// Original canonical report payload digest.
    #[must_use]
    pub const fn predecessor_report_digest(&self) -> Sha256Digest {
        self.predecessor_report_digest
    }
    /// Original finalized report artifact digest.
    #[must_use]
    pub const fn predecessor_report_artifact(&self) -> Sha256Digest {
        self.predecessor_report_artifact
    }
    /// Original admitted report evidence identity.
    #[must_use]
    pub const fn predecessor_evidence_id(&self) -> EvidenceId {
        self.predecessor_evidence_id
    }
    /// Original accepted report operation receipt.
    #[must_use]
    pub const fn predecessor_report_operation(&self) -> EvaluationOperationReceipt {
        self.predecessor_report_operation
    }
    /// Original accepted publication operation receipt.
    #[must_use]
    pub const fn predecessor_publication_operation(&self) -> EvaluationOperationReceipt {
        self.predecessor_publication_operation
    }
    /// Original acknowledged publication directive identity.
    #[must_use]
    pub const fn predecessor_outbox_id(&self) -> OutboxId {
        self.predecessor_outbox_id
    }
    /// Original acknowledged publication ownership fence.
    #[must_use]
    pub const fn predecessor_fence(&self) -> u64 {
        self.predecessor_fence
    }
    /// Successor accepted report operation receipt.
    #[must_use]
    pub const fn successor_report_operation(&self) -> EvaluationOperationReceipt {
        self.successor_report_operation
    }
    /// Successor accepted publication operation receipt.
    #[must_use]
    pub const fn successor_publication_operation(&self) -> EvaluationOperationReceipt {
        self.successor_publication_operation
    }
    /// Successor acknowledged publication directive identity.
    #[must_use]
    pub const fn successor_outbox_id(&self) -> OutboxId {
        self.successor_outbox_id
    }
    /// Successor acknowledged publication ownership fence.
    #[must_use]
    pub const fn successor_fence(&self) -> u64 {
        self.successor_fence
    }
    /// Strict ordinary F0 evidence captured for the repaired successor.
    #[must_use]
    pub const fn successor(&self) -> &PublishedEvaluationEvidence {
        &self.successor
    }
    /// Digest binding both immutable ownership chains and the strict successor evidence.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn operation_receipt_digest(receipt: EvaluationOperationReceipt) -> Sha256Digest {
    let sequence = receipt.sequence().to_be_bytes();
    let base_present = [u8::from(receipt.base_request_digest().is_some())];
    let absent_digest = Sha256Digest::new([0; 32]);
    let base_request = receipt.base_request_digest().unwrap_or(absent_digest);
    let mode = [match receipt.mode() {
        peritus_eval::EvaluationCommitMode::Ordinary => 1,
        peritus_eval::EvaluationCommitMode::Claimed => 2,
        peritus_eval::EvaluationCommitMode::Settlement => 3,
        peritus_eval::EvaluationCommitMode::Legacy => 4,
    }];
    let claim = receipt.original_claim();
    let claim_present = [u8::from(claim.is_some())];
    let absent_outbox = [0_u8; 16];
    let absent_fence = 0_u64.to_be_bytes();
    let claim_outbox = claim.map(|value| value.outbox_id());
    let claim_outbox = claim_outbox.as_ref().map_or(&absent_outbox, OutboxId::as_bytes);
    let claim_fence = claim.map_or(absent_fence, |value| value.fence().to_be_bytes());
    digest_parts(
        b"peritus.f0.evaluation-operation-receipt.v1\0",
        &[
            receipt.command_id().as_bytes(),
            receipt.event_id().as_bytes(),
            receipt.campaign_id().as_bytes(),
            &sequence,
            receipt.command_digest().as_bytes(),
            &base_present,
            base_request.as_bytes(),
            receipt.request_digest().as_bytes(),
            receipt.event_frame_digest().as_bytes(),
            receipt.successor_state_digest().as_bytes(),
            &mode,
            &claim_present,
            claim_outbox,
            &claim_fence,
        ],
    )
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
