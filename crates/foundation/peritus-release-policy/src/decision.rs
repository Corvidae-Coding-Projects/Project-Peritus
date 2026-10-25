//! Canonical assessments, diagnostics, and non-authorizing release verdict.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::ReleaseCandidate;

mod assessment;
mod completeness;
mod diagnostic;
mod digest;
mod finding;
mod review;
pub use self::assessment::{CriterionAssessment, EvidenceAssessment, QualificationAssessment};
use self::completeness::{criteria_complete, evidence_complete, qualifications_complete};
pub use self::diagnostic::Diagnostic;
use self::digest::decision_digest;
pub use self::finding::FindingAssessment;
pub use self::review::ReviewAssessment;
#[doc = " Explicit fail-closed H4 verdict."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReleaseVerdict {
    #[doc = " Every H4 policy obligation is satisfied for the exact candidate."]
    Ready,
    #[doc = " At least one production obligation is not satisfied."]
    NotReadyForProduction,
}
#[doc = " Stable deterministic decision fingerprint."]
#[doc = ""]
#[doc = " This is a domain-specific policy fingerprint, not a cryptographic signature. Publication"]
#[doc = " systems must retain and authenticate the canonical decision artifact separately."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DecisionDigest([u8; 32]);
impl DecisionDigest {
    pub(crate) const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    #[doc = " Returns the exact fingerprint bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
#[doc = " Pure H4 decision for one exact release candidate."]
#[doc = ""]
#[doc = " Construction is private. Callers cannot mint `Ready`, and `Ready` grants no publication,"]
#[doc = " signing, tagging, upload, deployment, or production-pointer authority."]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_release_policy::{ReleaseDecision, ReleaseVerdict};"]
#[doc = " let forged = ReleaseDecision { verdict: ReleaseVerdict::Ready };"]
#[doc = " ```"]
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
    ) -> Self {
        let complete = criteria_complete(&criteria)
            && evidence_complete(&evidence)
            && qualifications_complete(&qualifications)
            && reviews.is_satisfied()
            && findings.is_satisfied()
            && diagnostics.is_empty();
        let verdict =
            if complete { ReleaseVerdict::Ready } else { ReleaseVerdict::NotReadyForProduction };
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
    #[doc = " Returns the exact evaluated candidate."]
    #[must_use]
    pub const fn candidate(&self) -> ReleaseCandidate {
        self.candidate
    }
    #[doc = " Returns the monotonic policy-evaluation tick."]
    #[must_use]
    pub const fn evaluated_at(&self) -> u64 {
        self.evaluated_at
    }
    #[doc = " Returns the explicit fail-closed verdict."]
    #[must_use]
    pub const fn verdict(&self) -> ReleaseVerdict {
        self.verdict
    }
    #[doc = " Returns `true` exactly for [`ReleaseVerdict::Ready`]."]
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        (matches!(self.verdict, ReleaseVerdict::Ready)
            && criteria_complete(&self.criteria)
            && evidence_complete(&self.evidence)
            && qualifications_complete(&self.qualifications)
            && self.reviews.is_satisfied()
            && self.findings.is_satisfied()
            && self.diagnostics.is_empty())
    }
    #[doc = " Returns the stable deterministic decision fingerprint."]
    #[must_use]
    pub const fn digest(&self) -> DecisionDigest {
        self.digest
    }
    #[doc = " Returns all criterion assessments in stable ID order."]
    #[must_use]
    pub const fn criteria(&self) -> &[CriterionAssessment; 25] {
        &self.criteria
    }
    #[doc = " Returns all evidence assessments in stable requirement order."]
    #[must_use]
    pub const fn evidence(&self) -> &[EvidenceAssessment; 44] {
        &self.evidence
    }
    #[doc = " Returns H0-H3 assessments in canonical order."]
    #[must_use]
    pub const fn qualifications(&self) -> &[QualificationAssessment; 4] {
        &self.qualifications
    }
    #[doc = " Returns the independent-review assessment."]
    #[must_use]
    pub const fn reviews(&self) -> ReviewAssessment {
        self.reviews
    }
    #[doc = " Returns the finding and waiver assessment."]
    #[must_use]
    pub const fn findings(&self) -> FindingAssessment {
        self.findings
    }
    #[doc = " Returns diagnostics in canonical policy order."]
    #[must_use]
    pub const fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostics.as_slice()
    }
}
