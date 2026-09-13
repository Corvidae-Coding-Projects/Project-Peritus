//! Private-construction H0 decision and deterministic unmet conditions.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{
    AcceptanceCriterion, EvidenceArtifactKind, FindingSeverity, InventoryKind, ReviewScope,
    SecurityControlOutcome, SecurityRequirement,
};
use peritus_types::FindingId;
use vstd::prelude::*;

verus! {

/// Observation family whose exact candidate binding failed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationClass {
    Requirement,
    Criterion,
    Inventory,
    Artifact,
    ExternalReview,
    Finding,
}

/// One precise reason H0 readiness was withheld.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UnmetSecurityCondition {
    CandidateMismatch { class: ObservationClass, index: usize },
    MissingRequirement(SecurityRequirement),
    RequirementDidNotPass {
        requirement: SecurityRequirement,
        outcome: SecurityControlOutcome,
    },
    EmptyRequirementEvidence(SecurityRequirement),
    MissingCriterion(AcceptanceCriterion),
    CriterionDidNotPass {
        criterion: AcceptanceCriterion,
        outcome: SecurityControlOutcome,
    },
    EmptyCriterionEvidence(AcceptanceCriterion),
    MissingInventory(InventoryKind),
    InventoryIncomplete(InventoryKind),
    EmptyInventoryDigest(InventoryKind),
    MissingExternalReview,
    ExternalReviewIncomplete,
    ExternalReviewNotIndependent,
    EmptyExternalReviewDigest,
    MissingExternalReviewScope(ReviewScope),
    UnresolvedReleaseBlocker {
        finding_id: FindingId,
        severity: FindingSeverity,
    },
    MissingEvidenceArtifact(EvidenceArtifactKind),
    EmptyEvidenceDigest(EvidenceArtifactKind),
}

/// H0 security policy disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityVerdict {
    /// Every H0 obligation is complete for the exact candidate.
    Ready,
    /// At least one H0 obligation remains unmet.
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
    pub(super) const fn from_bool(value: bool) -> (result: Self)
        ensures result.spec_complete() == value,
    {
        if value { Self::Complete } else { Self::Incomplete }
    }

    pub(super) open spec fn spec_complete(self) -> bool { self == Self::Complete }

    pub(super) const fn is_complete(self) -> (complete: bool)
        ensures complete == self.spec_complete(),
    {
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
    ) -> (checks: Self)
        ensures checks.spec_complete() == crate::model::security_ready(
            candidate_bound.spec_complete(),
            requirements_complete.spec_complete(),
            criteria_complete.spec_complete(),
            inventories_complete.spec_complete(),
            independent_review_complete.spec_complete(),
            blockers_clear.spec_complete(),
            evidence_complete.spec_complete(),
        ),
    {
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

    pub(super) closed spec fn spec_complete(&self) -> bool {
        crate::model::security_ready(
            self.candidate_bound.spec_complete(),
            self.requirements_complete.spec_complete(),
            self.criteria_complete.spec_complete(),
            self.inventories_complete.spec_complete(),
            self.independent_review_complete.spec_complete(),
            self.blockers_clear.spec_complete(),
            self.evidence_complete.spec_complete(),
        )
    }

    pub(super) closed spec fn spec_candidate_bound(&self) -> bool {
        self.candidate_bound.spec_complete()
    }

    pub(super) closed spec fn spec_requirements_complete(&self) -> bool {
        self.requirements_complete.spec_complete()
    }

    pub(super) closed spec fn spec_criteria_complete(&self) -> bool {
        self.criteria_complete.spec_complete()
    }

    pub(super) closed spec fn spec_inventories_complete(&self) -> bool {
        self.inventories_complete.spec_complete()
    }

    pub(super) closed spec fn spec_independent_review_complete(&self) -> bool {
        self.independent_review_complete.spec_complete()
    }

    pub(super) closed spec fn spec_blockers_clear(&self) -> bool {
        self.blockers_clear.spec_complete()
    }

    pub(super) closed spec fn spec_evidence_complete(&self) -> bool {
        self.evidence_complete.spec_complete()
    }

    pub(super) const fn is_complete(self) -> (complete: bool)
        ensures complete == self.spec_complete(),
    {
        self.candidate_bound.is_complete()
            && self.requirements_complete.is_complete()
            && self.criteria_complete.is_complete()
            && self.inventories_complete.is_complete()
            && self.independent_review_complete.is_complete()
            && self.blockers_clear.is_complete()
            && self.evidence_complete.is_complete()
    }
}

/// Deterministic H0 security-readiness result.
///
/// Construction is private. The decision neither authorizes nor performs an H4 release.
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
    ) -> (decision: Self)
        ensures
            decision.spec_is_ready() ==> checks.spec_complete(),
            decision.spec_is_ready() ==> decision.spec_unmet_conditions().len() == 0,
            decision.spec_unmet_conditions() == unmet@,
    {
        let ready = unmet.is_empty() && checks.is_complete();
        let verdict = if ready { SecurityVerdict::Ready } else { SecurityVerdict::NotReady };
        let decision = Self { verdict, unmet, checks };
        reveal(SecurityDecision::spec_is_ready);
        decision
    }

    /// Returns the H0-only readiness disposition.
    #[must_use]
    pub const fn verdict(&self) -> SecurityVerdict { self.verdict }

    /// Returns true exactly when every phase is complete and no unmet condition remains.
    #[must_use]
    pub const fn is_ready(&self) -> (ready: bool)
        ensures ready == self.spec_is_ready(),
    {
        matches!(self.verdict, SecurityVerdict::Ready)
            && self.unmet.is_empty()
            && self.checks.is_complete()
    }

    /// Borrows unmet conditions in deterministic evaluator order.
    #[must_use]
    pub const fn unmet_conditions(&self) -> &[UnmetSecurityCondition] { self.unmet.as_slice() }

    /// Specification view of H0 readiness.
    pub closed spec fn spec_is_ready(&self) -> bool {
        self.verdict == SecurityVerdict::Ready
            && self.unmet@.len() == 0
            && self.checks.spec_complete()
    }

    /// Specification view of unmet conditions.
    pub closed spec fn spec_unmet_conditions(&self) -> Seq<UnmetSecurityCondition> { self.unmet@ }

    /// Specification view of all seven readiness phase checks.
    pub closed spec fn spec_checks_complete(&self) -> bool { self.checks.spec_complete() }

    /// Specification view of exact integrated-candidate binding.
    pub closed spec fn spec_candidate_bound(&self) -> bool {
        self.checks.spec_candidate_bound()
    }

    /// Specification view of complete R-SEC-001 through R-SEC-007 controls.
    pub closed spec fn spec_requirements_complete(&self) -> bool {
        self.checks.spec_requirements_complete()
    }

    /// Specification view of complete numbered acceptance criteria.
    pub closed spec fn spec_criteria_complete(&self) -> bool {
        self.checks.spec_criteria_complete()
    }

    /// Specification view of complete threat, control, unsafe, and TCB inventories.
    pub closed spec fn spec_inventories_complete(&self) -> bool {
        self.checks.spec_inventories_complete()
    }

    /// Specification view of complete independent external review.
    pub closed spec fn spec_independent_review_complete(&self) -> bool {
        self.checks.spec_independent_review_complete()
    }

    /// Specification view of release-blocking finding closure.
    pub closed spec fn spec_blockers_clear(&self) -> bool {
        self.checks.spec_blockers_clear()
    }

    /// Specification view of complete canonical evidence roles.
    pub closed spec fn spec_evidence_complete(&self) -> bool {
        self.checks.spec_evidence_complete()
    }

    pub(crate) proof fn ready_has_complete_checks(&self)
        requires self.spec_is_ready(),
        ensures self.spec_checks_complete(),
    {
        reveal(SecurityDecision::spec_is_ready);
        reveal(SecurityDecision::spec_checks_complete);
    }

    pub(crate) proof fn ready_has_no_unmet_conditions(&self)
        requires self.spec_is_ready(),
        ensures self.spec_unmet_conditions().len() == 0,
    {
        reveal(SecurityDecision::spec_is_ready);
        reveal(SecurityDecision::spec_unmet_conditions);
    }

    pub(crate) proof fn ready_has_all_security_obligations(&self)
        requires self.spec_is_ready(),
        ensures
            self.spec_candidate_bound(),
            self.spec_requirements_complete(),
            self.spec_criteria_complete(),
            self.spec_inventories_complete(),
            self.spec_independent_review_complete(),
            self.spec_blockers_clear(),
            self.spec_evidence_complete(),
    {
        reveal(SecurityDecision::spec_is_ready);
        reveal(SecurityDecision::spec_candidate_bound);
        reveal(SecurityDecision::spec_requirements_complete);
        reveal(SecurityDecision::spec_criteria_complete);
        reveal(SecurityDecision::spec_inventories_complete);
        reveal(SecurityDecision::spec_independent_review_complete);
        reveal(SecurityDecision::spec_blockers_clear);
        reveal(SecurityDecision::spec_evidence_complete);
        reveal(SecurityChecks::spec_complete);
        reveal(crate::model::security_ready);
    }
}

} // verus!
