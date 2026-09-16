//! Canonical assessments, diagnostics, and non-authorizing release verdict.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::ReleaseCandidate;
use vstd::prelude::*;

verus! {

mod assessment;
mod completeness;
mod diagnostic;
mod digest;
mod finding;
mod review;

pub use self::assessment::{CriterionAssessment, EvidenceAssessment, QualificationAssessment};
pub use self::diagnostic::Diagnostic;
pub use self::finding::FindingAssessment;
pub use self::review::ReviewAssessment;

use self::completeness::{criteria_complete, evidence_complete, qualifications_complete};
#[cfg(verus_only)]
pub(crate) use self::completeness::{
    evaluation_components_ready, spec_criteria_complete, spec_evidence_complete,
    spec_qualifications_complete,
};
#[cfg(verus_only)]
pub(crate) use self::digest::expected_decision_digest;
use self::digest::decision_digest;

/// Explicit fail-closed H4 verdict.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReleaseVerdict {
    /// Every H4 policy obligation is satisfied for the exact candidate.
    Ready,
    /// At least one production obligation is not satisfied.
    NotReadyForProduction,
}

/// Stable deterministic decision fingerprint.
///
/// This is a domain-specific policy fingerprint, not a cryptographic signature. Publication
/// systems must retain and authenticate the canonical decision artifact separately.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DecisionDigest([u8; 32]);

impl DecisionDigest {
    pub(crate) const fn new(bytes: [u8; 32]) -> (digest: Self)
        ensures digest.spec_bytes() == bytes
    { Self(bytes) }

    /// Logical view of every fingerprint byte.
    pub closed spec fn spec_bytes(&self) -> [u8; 32] { self.0 }

    /// Returns the exact fingerprint bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 32])
        ensures *bytes == self.spec_bytes()
    { &self.0 }
}

/// Pure H4 decision for one exact release candidate.
///
/// Construction is private. Callers cannot mint `Ready`, and `Ready` grants no publication,
/// signing, tagging, upload, deployment, or production-pointer authority.
///
/// ```compile_fail
/// use peritus_release_policy::{ReleaseDecision, ReleaseVerdict};
/// let forged = ReleaseDecision { verdict: ReleaseVerdict::Ready };
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct ReleaseDecision {
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    verdict: ReleaseVerdict,
    digest: DecisionDigest,
    criteria: [CriterionAssessment; 25],
    evidence: [EvidenceAssessment; 44],
    qualifications: [QualificationAssessment; 4],
    reviews: ReviewAssessment,
    findings: FindingAssessment,
    diagnostics: Vec<Diagnostic>,
}

impl ReleaseDecision {
    #[allow(
        clippy::large_types_passed_by_value,
        clippy::too_many_arguments,
        reason = "the evaluator transfers owned immutable decision components exactly once"
    )]
    pub(crate) const fn from_evaluation(
        candidate: ReleaseCandidate,
        evaluated_at: u64,
        criteria: [CriterionAssessment; 25],
        evidence: [EvidenceAssessment; 44],
        qualifications: [QualificationAssessment; 4],
        reviews: ReviewAssessment,
        findings: FindingAssessment,
        diagnostics: Vec<Diagnostic>,
    ) -> (decision: Self)
        ensures
            decision.spec_candidate() == candidate,
            decision.spec_evaluated_at() == evaluated_at,
            decision.spec_all_criteria_satisfied() == spec_criteria_complete(&criteria),
            decision.spec_required_artifacts_complete() == spec_evidence_complete(&evidence),
            decision.spec_all_qualifications_ready()
                == spec_qualifications_complete(&qualifications),
            decision.spec_reviews_complete() == reviews.spec_is_satisfied(),
            decision.spec_blockers_absent() == findings.spec_is_satisfied(),
            decision.spec_criteria() == criteria@,
            decision.spec_evidence() == evidence@,
            decision.spec_qualifications() == qualifications@,
            decision.spec_reviews() == reviews,
            decision.spec_findings() == findings,
            decision.spec_diagnostics() == diagnostics@,
            decision.spec_digest().spec_bytes()@ == expected_decision_digest(
                candidate.spec_manifest_digest(),
                decision.spec_verdict(),
                evidence@,
                qualifications@,
                reviews,
                findings,
            ),
            (decision.spec_verdict() == ReleaseVerdict::Ready)
                == evaluation_components_ready(
                    &criteria,
                    &evidence,
                    &qualifications,
                    reviews,
                    findings,
                    diagnostics@,
                ),
            decision.spec_is_ready() == evaluation_components_ready(
                &criteria,
                &evidence,
                &qualifications,
                reviews,
                findings,
                diagnostics@,
            ),
    {
        let complete = criteria_complete(&criteria)
            && evidence_complete(&evidence)
            && qualifications_complete(&qualifications)
            && reviews.is_satisfied()
            && findings.is_satisfied()
            && diagnostics.is_empty();
        let verdict = if complete {
            ReleaseVerdict::Ready
        } else {
            ReleaseVerdict::NotReadyForProduction
        };
        let digest = decision_digest(
            candidate.manifest_digest(),
            verdict,
            &evidence,
            &qualifications,
            reviews,
            findings,
        );
        let decision = Self {
            candidate,
            evaluated_at,
            verdict,
            digest,
            criteria,
            evidence,
            qualifications,
            reviews,
            findings,
            diagnostics,
        };
        proof {
            reveal(ReleaseDecision::spec_candidate);
            reveal(ReleaseDecision::spec_evaluated_at);
            reveal(ReleaseDecision::spec_verdict);
            reveal(ReleaseDecision::spec_is_ready);
            reveal(ReleaseDecision::spec_all_criteria_satisfied);
            reveal(ReleaseDecision::spec_required_artifacts_complete);
            reveal(ReleaseDecision::spec_all_qualifications_ready);
            reveal(ReleaseDecision::spec_reviews_complete);
            reveal(ReleaseDecision::spec_blockers_absent);
            reveal(ReleaseDecision::spec_criteria);
            reveal(ReleaseDecision::spec_evidence);
            reveal(ReleaseDecision::spec_qualifications);
            reveal(ReleaseDecision::spec_reviews);
            reveal(ReleaseDecision::spec_findings);
            reveal(ReleaseDecision::spec_diagnostics);
            reveal(evaluation_components_ready);
            reveal(spec_criteria_complete);
            reveal(spec_evidence_complete);
            reveal(spec_qualifications_complete);
        }
        decision
    }

    /// Returns the exact evaluated candidate.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: ReleaseCandidate)
        ensures candidate == self.spec_candidate()
    {
        self.candidate
    }

    /// Returns the monotonic policy-evaluation tick.
    #[must_use]
    pub const fn evaluated_at(&self) -> (evaluated_at: u64)
        ensures evaluated_at == self.spec_evaluated_at()
    {
        self.evaluated_at
    }

    /// Returns the explicit fail-closed verdict.
    #[must_use]
    pub const fn verdict(&self) -> (verdict: ReleaseVerdict)
        ensures verdict == self.spec_verdict()
    {
        self.verdict
    }

    /// Returns `true` exactly for [`ReleaseVerdict::Ready`].
    #[must_use]
    pub const fn is_ready(&self) -> (ready: bool)
        ensures ready == self.spec_is_ready()
    {
        let ready = matches!(self.verdict, ReleaseVerdict::Ready)
            && criteria_complete(&self.criteria)
            && evidence_complete(&self.evidence)
            && qualifications_complete(&self.qualifications)
            && self.reviews.is_satisfied()
            && self.findings.is_satisfied()
            && self.diagnostics.is_empty();
        proof {
            reveal(ReleaseDecision::spec_is_ready);
            reveal(ReleaseDecision::spec_all_criteria_satisfied);
            reveal(ReleaseDecision::spec_required_artifacts_complete);
            reveal(ReleaseDecision::spec_all_qualifications_ready);
            reveal(ReleaseDecision::spec_reviews_complete);
            reveal(ReleaseDecision::spec_blockers_absent);
            reveal(ReleaseDecision::spec_diagnostics);
        }
        ready
    }

    /// Returns the stable deterministic decision fingerprint.
    #[must_use]
    pub const fn digest(&self) -> (digest: DecisionDigest)
        ensures digest == self.spec_digest()
    { self.digest }

    /// Returns all criterion assessments in stable ID order.
    #[must_use]
    pub const fn criteria(&self) -> (criteria: &[CriterionAssessment; 25])
        ensures criteria@ == self.spec_criteria()
    { &self.criteria }

    /// Returns all evidence assessments in stable requirement order.
    #[must_use]
    pub const fn evidence(&self) -> (evidence: &[EvidenceAssessment; 44])
        ensures evidence@ == self.spec_evidence()
    { &self.evidence }

    /// Returns H0-H3 assessments in canonical order.
    #[must_use]
    pub const fn qualifications(&self) -> (qualifications: &[QualificationAssessment; 4])
        ensures qualifications@ == self.spec_qualifications()
    {
        &self.qualifications
    }

    /// Returns the independent-review assessment.
    #[must_use]
    pub const fn reviews(&self) -> (reviews: ReviewAssessment)
        ensures reviews == self.spec_reviews()
    { self.reviews }

    /// Returns the finding and waiver assessment.
    #[must_use]
    pub const fn findings(&self) -> (findings: FindingAssessment)
        ensures findings == self.spec_findings()
    { self.findings }

    /// Returns diagnostics in canonical policy order.
    #[must_use]
    pub const fn diagnostics(&self) -> (diagnostics: &[Diagnostic])
        ensures diagnostics@ == self.spec_diagnostics()
    { self.diagnostics.as_slice() }

    /// Specification view of the exact evaluated candidate.
    pub closed spec fn spec_candidate(&self) -> ReleaseCandidate { self.candidate }

    /// Specification view of the monotonic evaluation tick.
    pub closed spec fn spec_evaluated_at(&self) -> u64 { self.evaluated_at }

    /// Specification view of the raw fail-closed verdict stored by the evaluator.
    pub closed spec fn spec_verdict(&self) -> ReleaseVerdict { self.verdict }

    /// Specification view of the deterministic decision fingerprint.
    pub closed spec fn spec_digest(&self) -> DecisionDigest { self.digest }

    /// Specification view of all criterion assessments in stable order.
    pub closed spec fn spec_criteria(&self) -> Seq<CriterionAssessment> { self.criteria@ }

    /// Specification view of all evidence assessments in stable order.
    pub closed spec fn spec_evidence(&self) -> Seq<EvidenceAssessment> { self.evidence@ }

    /// Specification view of H0-H3 assessments in stable order.
    pub closed spec fn spec_qualifications(&self) -> Seq<QualificationAssessment> {
        self.qualifications@
    }

    /// Specification view of the independent-review assessment.
    pub closed spec fn spec_reviews(&self) -> ReviewAssessment { self.reviews }

    /// Specification view of the finding and waiver assessment.
    pub closed spec fn spec_findings(&self) -> FindingAssessment { self.findings }

    /// Specification view of the final ready verdict.
    pub closed spec fn spec_is_ready(&self) -> bool {
        self.verdict == ReleaseVerdict::Ready
            && self.spec_all_criteria_satisfied()
            && self.spec_required_artifacts_complete()
            && self.spec_all_qualifications_ready()
            && self.spec_reviews_complete()
            && self.spec_blockers_absent()
            && self.spec_diagnostics().len() == 0
    }

    /// Specification view of all twenty-five criterion assessments.
    pub closed spec fn spec_all_criteria_satisfied(&self) -> bool {
        spec_criteria_complete(&self.criteria)
    }

    /// Specification view of exact-ready H0-H3 inputs.
    pub closed spec fn spec_all_qualifications_ready(&self) -> bool {
        spec_qualifications_complete(&self.qualifications)
    }

    /// Specification view of required artifact completeness.
    pub closed spec fn spec_required_artifacts_complete(&self) -> bool {
        spec_evidence_complete(&self.evidence)
    }

    /// Specification view of independent-review completeness.
    pub closed spec fn spec_reviews_complete(&self) -> bool {
        self.reviews.spec_is_satisfied()
    }

    /// Specification view of blocker absence and waiver validity.
    pub closed spec fn spec_blockers_absent(&self) -> bool {
        self.findings.spec_is_satisfied()
    }

    /// Specification view of canonical diagnostics.
    pub closed spec fn spec_diagnostics(&self) -> Seq<Diagnostic> { self.diagnostics@ }

    /// Proves that the executable ready state carries every final policy obligation.
    pub proof fn ready_implies_final_obligations(&self)
        requires self.spec_is_ready(),
        ensures
            self.spec_all_criteria_satisfied(),
            self.spec_all_qualifications_ready(),
            self.spec_required_artifacts_complete(),
            self.spec_reviews_complete(),
            self.spec_blockers_absent(),
            self.spec_diagnostics().len() == 0,
    {
        reveal(ReleaseDecision::spec_is_ready);
        reveal(ReleaseDecision::spec_all_criteria_satisfied);
        reveal(ReleaseDecision::spec_all_qualifications_ready);
        reveal(ReleaseDecision::spec_required_artifacts_complete);
        reveal(ReleaseDecision::spec_reviews_complete);
        reveal(ReleaseDecision::spec_blockers_absent);
        reveal(ReleaseDecision::spec_diagnostics);
    }
}

} // verus!
