//! Private-construction H0 decision and deterministic unmet conditions.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    AcceptanceCriterion, EvidenceArtifactKind, FindingSeverity, InventoryKind, ReviewScope,
    SecurityControlOutcome, SecurityRequirement,
};
use peritus_types::FindingId;

#[doc = " Observation family whose exact candidate binding failed."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationClass {
    Requirement,
    Criterion,
    Inventory,
    Artifact,
    ExternalReview,
    Finding,
}
#[doc = " One precise reason H0 readiness was withheld."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UnmetSecurityCondition {
    CandidateMismatch { class: ObservationClass, index: usize },
    MissingRequirement(SecurityRequirement),
    RequirementDidNotPass { requirement: SecurityRequirement, outcome: SecurityControlOutcome },
    EmptyRequirementEvidence(SecurityRequirement),
    MissingCriterion(AcceptanceCriterion),
    CriterionDidNotPass { criterion: AcceptanceCriterion, outcome: SecurityControlOutcome },
    EmptyCriterionEvidence(AcceptanceCriterion),
    MissingInventory(InventoryKind),
    InventoryIncomplete(InventoryKind),
    EmptyInventoryDigest(InventoryKind),
    MissingExternalReview,
    ExternalReviewIncomplete,
    ExternalReviewNotIndependent,
    EmptyExternalReviewDigest,
    MissingExternalReviewScope(ReviewScope),
    UnresolvedReleaseBlocker { finding_id: FindingId, severity: FindingSeverity },
    MissingEvidenceArtifact(EvidenceArtifactKind),
    EmptyEvidenceDigest(EvidenceArtifactKind),
}
#[doc = " H0 security policy disposition."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityVerdict {
    #[doc = " Every H0 obligation is complete for the exact candidate."]
    Ready,
    #[doc = " At least one H0 obligation remains unmet."]
    NotReady,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evaluator modules require the private phase result"
)]
pub(super) enum CheckResult {
    Complete,
    Incomplete,
}
impl CheckResult {
    pub(super) const fn from_bool(value: bool) -> Self {
        if value { Self::Complete } else { Self::Incomplete }
    }
    pub(super) const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evaluator modules require the private readiness aggregate"
)]
pub(super) struct SecurityChecks {
    candidate_bound: CheckResult,
    requirements_complete: CheckResult,
    criteria_complete: CheckResult,
    inventories_complete: CheckResult,
    independent_review_complete: CheckResult,
    blockers_clear: CheckResult,
    evidence_complete: CheckResult,
}
impl SecurityChecks {
    #[allow(clippy::too_many_arguments, reason = "every H0 readiness obligation remains explicit")]
    pub(super) const fn new(
        candidate_bound: CheckResult,
        requirements_complete: CheckResult,
        criteria_complete: CheckResult,
        inventories_complete: CheckResult,
        independent_review_complete: CheckResult,
        blockers_clear: CheckResult,
        evidence_complete: CheckResult,
    ) -> Self {
        Self {
            candidate_bound,
            requirements_complete,
            criteria_complete,
            inventories_complete,
            independent_review_complete,
            blockers_clear,
            evidence_complete,
        }
    }
    pub(super) const fn is_complete(self) -> bool {
        self.candidate_bound.is_complete()
            && self.requirements_complete.is_complete()
            && self.criteria_complete.is_complete()
            && self.inventories_complete.is_complete()
            && self.independent_review_complete.is_complete()
            && self.blockers_clear.is_complete()
            && self.evidence_complete.is_complete()
    }
}
#[doc = " Deterministic H0 security-readiness result."]
#[doc = ""]
#[doc = " Construction is private. The decision neither authorizes nor performs an H4 release."]
#[derive(Debug, Eq, PartialEq)]
pub struct SecurityDecision {
    verdict: SecurityVerdict,
    unmet: Vec<UnmetSecurityCondition>,
    checks: SecurityChecks,
}
impl SecurityDecision {
    pub(super) const fn from_evaluation(
        unmet: Vec<UnmetSecurityCondition>,
        checks: SecurityChecks,
    ) -> Self {
        let ready = unmet.is_empty() && checks.is_complete();
        let verdict = if ready { SecurityVerdict::Ready } else { SecurityVerdict::NotReady };

        Self { verdict, unmet, checks }
    }
    #[doc = " Returns the H0-only readiness disposition."]
    #[must_use]
    pub const fn verdict(&self) -> SecurityVerdict {
        self.verdict
    }
    #[doc = " Returns true exactly when every phase is complete and no unmet condition remains."]
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self.verdict, SecurityVerdict::Ready)
            && self.unmet.is_empty()
            && self.checks.is_complete()
    }
    #[doc = " Borrows unmet conditions in deterministic evaluator order."]
    #[must_use]
    pub const fn unmet_conditions(&self) -> &[UnmetSecurityCondition] {
        self.unmet.as_slice()
    }
}
