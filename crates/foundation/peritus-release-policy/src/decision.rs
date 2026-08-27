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

pub use self::assessment::{CriterionAssessment, EvidenceAssessment, QualificationAssessment};
pub use self::diagnostic::Diagnostic;
pub use self::finding::FindingAssessment;

use self::completeness::{criteria_complete, evidence_complete, qualifications_complete};
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
    pub(crate) const fn new(bytes: [u8; 32]) -> Self { Self(bytes) }

    /// Returns the exact fingerprint bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}

/// Aggregated independent-review state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "review independence dimensions must remain explicit")]
pub struct ReviewAssessment {
    satisfied: bool,
    approved_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    changes_required_count: u16,
    self_review_count: u16,
    non_independent_count: u16,
    duplicate_reviewer: bool,
    shared_context: bool,
    conflicting_review: bool,
}

impl ReviewAssessment {
    #[allow(
        clippy::fn_params_excessive_bools,
        clippy::too_many_arguments,
        reason = "review independence dimensions remain explicit"
    )]
    pub(crate) const fn new(
        satisfied: bool,
        approved_count: u16,
        stale_count: u16,
        mismatched_count: u16,
        changes_required_count: u16,
        self_review_count: u16,
        non_independent_count: u16,
        duplicate_reviewer: bool,
        shared_context: bool,
        conflicting_review: bool,
    ) -> Self {
        Self {
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
        }
    }

    /// Returns whether the independent-review quorum is clean and complete.
    #[must_use]
    pub const fn is_satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_is_satisfied()
    {
        self.satisfied
    }

    /// Logical view of whether the independent-review quorum is clean and complete.
    pub closed spec fn spec_is_satisfied(&self) -> bool {
        self.satisfied
    }

    /// Returns the saturated approved-review count.
    #[must_use]
    pub const fn approved_count(&self) -> u16 { self.approved_count }

    /// Returns the saturated stale-review count.
    #[must_use]
    pub const fn stale_count(&self) -> u16 { self.stale_count }

    /// Returns the saturated mismatched-review count.
    #[must_use]
    pub const fn mismatched_count(&self) -> u16 { self.mismatched_count }

    /// Returns the saturated changes-required count.
    #[must_use]
    pub const fn changes_required_count(&self) -> u16 { self.changes_required_count }

    /// Returns the saturated self-review count.
    #[must_use]
    pub const fn self_review_count(&self) -> u16 { self.self_review_count }

    /// Returns the saturated non-independent-review count.
    #[must_use]
    pub const fn non_independent_count(&self) -> u16 { self.non_independent_count }

    /// Returns whether current reviews reused a reviewer identity.
    #[must_use]
    pub const fn has_duplicate_reviewer(&self) -> bool { self.duplicate_reviewer }

    /// Returns whether current reviews reused a fresh-context digest.
    #[must_use]
    pub const fn has_shared_context(&self) -> bool { self.shared_context }

    /// Returns whether observations with one review identity disagreed.
    #[must_use]
    pub const fn has_conflicting_review(&self) -> bool { self.conflicting_review }
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
    pub(crate) fn from_evaluation(
        candidate: ReleaseCandidate,
        evaluated_at: u64,
        criteria: [CriterionAssessment; 25],
        evidence: [EvidenceAssessment; 44],
        qualifications: [QualificationAssessment; 4],
        reviews: ReviewAssessment,
        findings: FindingAssessment,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
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
        Self {
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
        }
    }

    /// Returns the exact evaluated candidate.
    #[must_use]
    pub const fn candidate(&self) -> ReleaseCandidate { self.candidate }

    /// Returns the monotonic policy-evaluation tick.
    #[must_use]
    pub const fn evaluated_at(&self) -> u64 { self.evaluated_at }

    /// Returns the explicit fail-closed verdict.
    #[must_use]
    pub const fn verdict(&self) -> ReleaseVerdict { self.verdict }

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
    pub const fn digest(&self) -> DecisionDigest { self.digest }

    /// Returns all criterion assessments in stable ID order.
    #[must_use]
    pub const fn criteria(&self) -> &[CriterionAssessment; 25] { &self.criteria }

    /// Returns all evidence assessments in stable requirement order.
    #[must_use]
    pub const fn evidence(&self) -> &[EvidenceAssessment; 44] { &self.evidence }

    /// Returns H0-H3 assessments in canonical order.
    #[must_use]
    pub const fn qualifications(&self) -> &[QualificationAssessment; 4] {
        &self.qualifications
    }

    /// Returns the independent-review assessment.
    #[must_use]
    pub const fn reviews(&self) -> ReviewAssessment { self.reviews }

    /// Returns the finding and waiver assessment.
    #[must_use]
    pub const fn findings(&self) -> FindingAssessment { self.findings }

    /// Returns diagnostics in canonical policy order.
    #[must_use]
    pub const fn diagnostics(&self) -> &[Diagnostic] { self.diagnostics.as_slice() }

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
        self.criteria[0].spec_is_satisfied() && self.criteria[1].spec_is_satisfied()
            && self.criteria[2].spec_is_satisfied() && self.criteria[3].spec_is_satisfied()
            && self.criteria[4].spec_is_satisfied() && self.criteria[5].spec_is_satisfied()
            && self.criteria[6].spec_is_satisfied() && self.criteria[7].spec_is_satisfied()
            && self.criteria[8].spec_is_satisfied() && self.criteria[9].spec_is_satisfied()
            && self.criteria[10].spec_is_satisfied() && self.criteria[11].spec_is_satisfied()
            && self.criteria[12].spec_is_satisfied() && self.criteria[13].spec_is_satisfied()
            && self.criteria[14].spec_is_satisfied() && self.criteria[15].spec_is_satisfied()
            && self.criteria[16].spec_is_satisfied() && self.criteria[17].spec_is_satisfied()
            && self.criteria[18].spec_is_satisfied() && self.criteria[19].spec_is_satisfied()
            && self.criteria[20].spec_is_satisfied() && self.criteria[21].spec_is_satisfied()
            && self.criteria[22].spec_is_satisfied() && self.criteria[23].spec_is_satisfied()
            && self.criteria[24].spec_is_satisfied()
    }

    /// Specification view of exact-ready H0-H3 inputs.
    pub closed spec fn spec_all_qualifications_ready(&self) -> bool {
        self.qualifications[0].spec_is_satisfied()
            && self.qualifications[1].spec_is_satisfied()
            && self.qualifications[2].spec_is_satisfied()
            && self.qualifications[3].spec_is_satisfied()
    }

    /// Specification view of required artifact completeness.
    pub closed spec fn spec_required_artifacts_complete(&self) -> bool {
        self.evidence[0].spec_is_satisfied() && self.evidence[1].spec_is_satisfied()
            && self.evidence[2].spec_is_satisfied() && self.evidence[3].spec_is_satisfied()
            && self.evidence[4].spec_is_satisfied() && self.evidence[5].spec_is_satisfied()
            && self.evidence[6].spec_is_satisfied() && self.evidence[7].spec_is_satisfied()
            && self.evidence[8].spec_is_satisfied() && self.evidence[9].spec_is_satisfied()
            && self.evidence[10].spec_is_satisfied() && self.evidence[11].spec_is_satisfied()
            && self.evidence[12].spec_is_satisfied() && self.evidence[13].spec_is_satisfied()
            && self.evidence[14].spec_is_satisfied() && self.evidence[15].spec_is_satisfied()
            && self.evidence[16].spec_is_satisfied() && self.evidence[17].spec_is_satisfied()
            && self.evidence[18].spec_is_satisfied() && self.evidence[19].spec_is_satisfied()
            && self.evidence[20].spec_is_satisfied() && self.evidence[21].spec_is_satisfied()
            && self.evidence[22].spec_is_satisfied() && self.evidence[23].spec_is_satisfied()
            && self.evidence[24].spec_is_satisfied() && self.evidence[25].spec_is_satisfied()
            && self.evidence[26].spec_is_satisfied() && self.evidence[27].spec_is_satisfied()
            && self.evidence[28].spec_is_satisfied() && self.evidence[29].spec_is_satisfied()
            && self.evidence[30].spec_is_satisfied() && self.evidence[31].spec_is_satisfied()
            && self.evidence[32].spec_is_satisfied() && self.evidence[33].spec_is_satisfied()
            && self.evidence[34].spec_is_satisfied() && self.evidence[35].spec_is_satisfied()
            && self.evidence[36].spec_is_satisfied() && self.evidence[37].spec_is_satisfied()
            && self.evidence[38].spec_is_satisfied() && self.evidence[39].spec_is_satisfied()
            && self.evidence[40].spec_is_satisfied() && self.evidence[41].spec_is_satisfied()
            && self.evidence[42].spec_is_satisfied() && self.evidence[43].spec_is_satisfied()
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
