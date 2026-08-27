//! Total deterministic H4 release evaluation.

use crate::{
    AcceptanceCriterion, CriterionAssessment, Diagnostic, EvidenceAssessment,
    EvidenceRequirement, QualificationSlice, ReleaseCandidate, ReleaseDecision, ReleaseEvidence,
    ReviewAssessment, ReviewOutcome,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

mod diagnostics;
mod findings;
mod qualifications;

/// Minimum distinct independent approvals required by H4.
pub const MIN_INDEPENDENT_REVIEWERS: u16 = 2;

/// Formal contract established whenever [`evaluate_release`] reports ready.
pub open spec fn ready_evaluation_contract(decision: &ReleaseDecision) -> bool {
    decision.spec_is_ready() ==> {
        &&& decision.spec_all_criteria_satisfied()
        &&& decision.spec_required_artifacts_complete()
        &&& decision.spec_all_qualifications_ready()
        &&& decision.spec_reviews_complete()
        &&& decision.spec_blockers_absent()
        &&& decision.spec_diagnostics().len() == 0
    }
}

/// Evaluates all H4 production obligations for one exact release candidate.
///
/// The evaluator performs no I/O and grants no publication authority. Input order does not affect
/// output order: requirements, criteria, and H0-H3 inputs are always assessed in their closed
/// stable-ID order; review/finding diagnostics are aggregate facts rather than input positions.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the top-level phase order is intentionally visible and auditable")]
pub fn evaluate_release(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (decision: ReleaseDecision)
    ensures ready_evaluation_contract(&decision)
{
    let evidence_assessments = [
        assess_evidence(EvidenceRequirement::GateA, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::FoundationQualityMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::FoundationVerusVerify, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::FoundationVerusBuild, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ProofInventory, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::TrustBoundaryAudit, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::PrivilegedConstructionConformance, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::IllegalLifecycleEdgeMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::CrashInjectionCampaign, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::DeterministicReplayCorpus, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::MaliciousRepositorySuite, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::LinuxNativeQualification, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::MacOsNativeQualification, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::WindowsNativeQualification, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::SandboxEscapeReview, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::RoleIsolationMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::EvidenceInvalidationMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ExhaustionFailClosedMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::DaemonRecoveryCampaign, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ProviderContractMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::MigrationCorpus, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::EvidenceExport, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::EvolutionRedTeam, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::PromotionGateMatrix, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::AtomicRollback, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ObservabilityCitations, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::SecretRedaction, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::LoadSlo, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::EightHourSoak, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::PublicReferenceDocumentation, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::CommandProtocolEndToEnd, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ArchitectureAudit, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::RepresentativeCampaign, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ReproducibleArtifacts, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ArtifactSignatures, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::Sbom, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::Provenance, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::LicenseNotices, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::MigrationRecoveryDocumentation, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::CompletedSecurityReview, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::TestQuarantineAudit, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::ReleaseFindingAudit, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::UnsafeInventory, candidate, evaluated_at, evidence),
        assess_evidence(EvidenceRequirement::PlaceholderAudit, candidate, evaluated_at, evidence),
    ];

    let criteria = [
        assess_criterion(AcceptanceCriterion::CleanTierOneSuite, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::VerifiedWorkspaceBuild, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::ProofObligationInventory, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::TrustedConstructAudit, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::PrivilegedConstruction, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::IllegalLifecycleEdges, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::CrashRecovery, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::DeterministicReplay, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::MaliciousRepository, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::NativeSandboxSecurity, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::RoleIsolation, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::EvidenceInvalidation, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::ExhaustionFailsClosed, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::DaemonLifecycleRecovery, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::ProviderContracts, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::MigrationAndExport, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::EvolutionIsolation, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::PromotionAndRollback, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::ObservabilityAndRedaction, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::LoadAndSoak, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::PublicSurfaceDocumentation, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::ArchitectureIntegrity, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::RepresentativeCampaign, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::ReleaseArtifacts, &evidence_assessments),
        assess_criterion(AcceptanceCriterion::NoReleaseDebt, &evidence_assessments),
    ];

    let qualifications = [
        qualifications::assess(QualificationSlice::H0Security, candidate, evaluated_at, evidence),
        qualifications::assess(QualificationSlice::H1Resilience, candidate, evaluated_at, evidence),
        qualifications::assess(QualificationSlice::H2Platform, candidate, evaluated_at, evidence),
        qualifications::assess(QualificationSlice::H3Performance, candidate, evaluated_at, evidence),
    ];

    let reviews = assess_reviews(candidate, evaluated_at, evidence);
    let findings = findings::assess(candidate, evaluated_at, evidence);
    let mut diagnostics = Vec::<Diagnostic>::new();
    diagnostics::push_evidence(&evidence_assessments, &mut diagnostics);
    diagnostics::push_qualifications(&qualifications, &mut diagnostics);
    diagnostics::push_reviews(reviews, &mut diagnostics);
    diagnostics::push_findings(findings, &mut diagnostics);

    let decision = ReleaseDecision::from_evaluation(
        candidate,
        evaluated_at,
        criteria,
        evidence_assessments,
        qualifications,
        reviews,
        findings,
        diagnostics,
    );
    proof {
        reveal(ready_evaluation_contract);
        if decision.spec_is_ready() {
            decision.ready_implies_final_obligations();
        }
    }
    decision
}

#[allow(clippy::large_types_passed_by_value, reason = "exact candidate identity is a Copy policy value")]
fn assess_evidence(
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> EvidenceAssessment {
    let mut contributing_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut wrong_source_count = 0u16;
    let mut unreviewed_count = 0u16;
    let mut unsigned_count = 0u16;
    let mut conflicting = false;
    let mut first_digest = None::<Sha256Digest>;
    let mut aggregate = [0u8; 32];
    let mut index = 0;
    while index < evidence.observations().len()
        invariant 0 <= index <= evidence.spec_observations().len(),
        decreases evidence.spec_observations().len() - index,
    {
        let observation = evidence.observations()[index];
        if observation.requirement() == requirement {
            if observation.binding().is_mismatched(candidate) {
                increment(&mut mismatched_count);
            } else if observation.binding().is_stale_at(candidate, evaluated_at) {
                increment(&mut stale_count);
            } else if observation.source_kind() != requirement.source_kind() {
                increment(&mut wrong_source_count);
            } else {
                if !observation.reviewed() { increment(&mut unreviewed_count); }
                if !observation.signed() { increment(&mut unsigned_count); }
                if observation.reviewed() && observation.signed() {
                    increment(&mut contributing_count);
                    if let Some(previous) = first_digest {
                        if previous != observation.artifact_digest() { conflicting = true; }
                    } else {
                        first_digest = Some(observation.artifact_digest());
                    }
                    xor_digest(&mut aggregate, observation.artifact_digest());
                }
            }
        }
        index += 1;
    }
    let satisfied = contributing_count > 0
        && stale_count == 0
        && mismatched_count == 0
        && wrong_source_count == 0
        && unreviewed_count == 0
        && unsigned_count == 0
        && !conflicting;
    EvidenceAssessment::new(
        requirement,
        satisfied,
        contributing_count,
        stale_count,
        mismatched_count,
        wrong_source_count,
        unreviewed_count,
        unsigned_count,
        conflicting,
        Sha256Digest::new(aggregate),
    )
}

fn assess_criterion(
    criterion: AcceptanceCriterion,
    assessments: &[EvidenceAssessment; 44],
) -> CriterionAssessment {
    let mut satisfied = true;
    let mut found = false;
    let mut index = 0;
    while index < assessments.len()
        invariant 0 <= index <= assessments.len(),
        decreases assessments.len() - index,
    {
        if assessments[index].requirement().criterion() == criterion {
            found = true;
            if !assessments[index].is_satisfied() { satisfied = false; }
        }
        index += 1;
    }
    CriterionAssessment::new(criterion, found && satisfied)
}

#[allow(clippy::large_types_passed_by_value, reason = "exact candidate identity is a Copy policy value")]
fn assess_reviews(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> ReviewAssessment {
    let mut approved_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut changes_required_count = 0u16;
    let mut self_review_count = 0u16;
    let mut non_independent_count = 0u16;
    let mut duplicate_reviewer = false;
    let mut shared_context = false;
    let mut conflicting_review = false;

    let mut right = 0;
    while right < evidence.reviews().len()
        invariant 0 <= right <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - right,
    {
        let review = evidence.reviews()[right];
        if review.binding().is_mismatched(candidate) {
            increment(&mut mismatched_count);
        } else if review.binding().is_stale_at(candidate, evaluated_at) {
            increment(&mut stale_count);
        } else {
            if review.reviewer() == review.producer() { increment(&mut self_review_count); }
            if !review.independent_from_producer() { increment(&mut non_independent_count); }
            match review.outcome() {
                ReviewOutcome::Approved => {
                    if review.reviewer() != review.producer()
                        && review.independent_from_producer()
                    {
                        increment(&mut approved_count);
                    }
                }
                ReviewOutcome::ChangesRequired => increment(&mut changes_required_count),
            }
            let mut left = 0;
            while left < right
                invariant
                    0 <= left <= right,
                    right < evidence.spec_reviews().len(),
                decreases right - left,
            {
                let previous = evidence.reviews()[left];
                if previous.binding().is_current_for(candidate, evaluated_at) {
                    if previous.reviewer() == review.reviewer() { duplicate_reviewer = true; }
                    if previous.context_digest() == review.context_digest() { shared_context = true; }
                    if previous.id() == review.id() && previous != review {
                        conflicting_review = true;
                    }
                }
                left += 1;
            }
        }
        right += 1;
    }

    let satisfied = approved_count >= MIN_INDEPENDENT_REVIEWERS
        && stale_count == 0
        && mismatched_count == 0
        && changes_required_count == 0
        && self_review_count == 0
        && non_independent_count == 0
        && !duplicate_reviewer
        && !shared_context
        && !conflicting_review;
    ReviewAssessment::new(
        satisfied,
        approved_count,
        stale_count,
        mismatched_count,
        changes_required_count,
        self_review_count,
        non_independent_count,
        duplicate_reviewer,
        shared_context,
        conflicting_review,
    )
}

const fn increment(value: &mut u16) {
    *value = (*value).saturating_add(1);
}

const fn xor_digest(output: &mut [u8; 32], digest: Sha256Digest) {
    let mut index = 0;
    while index < output.len()
        invariant 0 <= index <= output.len(),
        decreases output.len() - index,
    {
        output[index] ^= digest.as_bytes()[index];
        index += 1;
    }
}

} // verus!
