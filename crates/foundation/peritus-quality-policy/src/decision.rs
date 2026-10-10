//! Deterministic acceptance decision and typed unmet conditions.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::GateFailure;
use peritus_spec::{EvidenceRequirementId, FindingSeverity, ReviewCategory};
use peritus_types::{ActorId, FindingId, GateId, ReviewCycleId};

#[doc = " Kind of exact-revision observation reported as stale."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationKind {
    #[doc = " Deterministic gate result."]
    Gate,
    #[doc = " Reviewer result."]
    Review,
    #[doc = " Required artifact."]
    Evidence,
    #[doc = " Human approval result."]
    Approval,
    #[doc = " Finding waiver."]
    Waiver,
}
#[doc = " Reviewer-independence rule not met by the current quorum."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewerIndependenceFailure {
    #[doc = " Two current reviews used the same reviewer actor."]
    DistinctReviewers,
    #[doc = " At least one reviewer was not independent from the producer."]
    ProducerIndependence,
    #[doc = " Two reviews used the same fresh-context identity."]
    DistinctContexts,
    #[doc = " Two reviews used the same model family."]
    DistinctModelFamilies,
    #[doc = " Two reviews used the same provider."]
    DistinctProviders,
    #[doc = " Two reviews declared the same causal ancestry."]
    SharedAncestry,
}
#[doc = " Reason a supplied waiver cannot resolve its finding."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvalidWaiverReason {
    #[doc = " The contract forbids waivers."]
    Forbidden,
    #[doc = " The finding was not explicitly dispositioned as waiver-requested."]
    NotRequested,
    #[doc = " The waiver named an authority other than the contract authority."]
    WrongAuthority,
    #[doc = " The waiver named the wrong evidence declaration."]
    WrongEvidenceRequirement,
    #[doc = " The waiver lacks a current matching approved human authority observation."]
    MissingApproval,
    #[doc = " The matching human authority explicitly denied the waiver."]
    ApprovalDenied,
    #[doc = " The waiver references a finding absent from current reviews."]
    UnknownFinding,
    #[doc = " The referenced waiver evidence artifact is absent or stale."]
    MissingEvidence,
    #[doc = " A waiver was supplied for a finding that was already resolved."]
    AlreadyResolved,
}
#[doc = " One precise reason the current revision is not acceptable."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnmetCondition {
    #[doc = " The requested tuple does not name the contract\'s exact acceptance-specification identity."]
    ContractRevisionMismatch,
    #[doc = " An observation belongs to another revision tuple."]
    StaleObservation {
        #[doc = " Observation collection."]
        kind: ObservationKind,
        #[doc = " Canonical zero-based position in that collection."]
        index: usize,
    },
    #[doc = " A gate observation names no gate in the contract."]
    UnknownGate(GateId),
    #[doc = " A required gate has no current observation."]
    MissingGate(GateId),
    #[doc = " A required current gate did not pass."]
    GateDidNotPass {
        #[doc = " Required gate."]
        gate_id: GateId,
        #[doc = " Normalized failure reason."]
        failure: GateFailure,
    },
    #[doc = " A passing gate observation was produced after the contract\'s attempt budget."]
    GateAttemptLimitExceeded {
        #[doc = " Required gate."]
        gate_id: GateId,
        #[doc = " One-based observed attempt."]
        attempt: u16,
        #[doc = " Contract maximum."]
        maximum: u16,
    },
    #[doc = " An evidence observation names no contract requirement."]
    UnknownEvidence(EvidenceRequirementId),
    #[doc = " A contract evidence requirement has no current artifact."]
    MissingEvidence(EvidenceRequirementId),
    #[doc = " A review claims a category not declared by the contract."]
    UnknownReviewCategory(ReviewCategory),
    #[doc = " No current review covers the required category."]
    MissingReviewCategory(ReviewCategory),
    #[doc = " Too few current reviews are present."]
    ReviewerQuorum {
        #[doc = " Contract-required review count."]
        required: u16,
        #[doc = " Number of current reviews, saturated to `u16::MAX`."]
        observed: u16,
    },
    #[doc = " A current review was produced after the contract\'s review-cycle budget."]
    ReviewCycleLimitExceeded {
        #[doc = " Stable review-cycle identity."]
        cycle_id: ReviewCycleId,
        #[doc = " One-based observed cycle ordinal."]
        cycle: u16,
        #[doc = " Contract maximum."]
        maximum: u16,
    },
    #[doc = " Current reviews violate a configured independence constraint."]
    ReviewerIndependence(ReviewerIndependenceFailure),
    #[doc = " A finding at or above the blocker threshold remains unresolved and unwaived."]
    UnwaivedBlocker {
        #[doc = " Stable finding identity."]
        finding_id: FindingId,
        #[doc = " Normalized severity."]
        severity: FindingSeverity,
    },
    #[doc = " A supplied waiver is unusable."]
    InvalidWaiver {
        #[doc = " Finding targeted by the waiver."]
        finding_id: FindingId,
        #[doc = " Exact reason the waiver failed."]
        reason: InvalidWaiverReason,
    },
    #[doc = " Final human acceptance approval is required but absent."]
    MissingHumanApproval,
    #[doc = " A supplied final human approval named the wrong authority policy."]
    WrongHumanApprovalAuthority,
    #[doc = " Human authority explicitly denied final acceptance."]
    HumanApprovalDenied,
    #[doc = " An approval observation is unrelated to a configured acceptance or supplied waiver."]
    UnexpectedApproval(ActorId),
}
#[doc = " Fail-closed status of one independently computed acceptance obligation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "sibling evaluator access is required while the decision module remains private"
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
#[doc = " Independently computed completeness facts carried by every evaluator result."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "sibling evaluator access is required while the decision module remains private"
)]
pub(super) struct AcceptanceChecks {
    contract_bound: CheckResult,
    observations_fresh: CheckResult,
    gates_complete: CheckResult,
    evidence_complete: CheckResult,
    reviews_complete: CheckResult,
    blockers_complete: CheckResult,
    approvals_complete: CheckResult,
}
impl AcceptanceChecks {
    #[allow(
        clippy::too_many_arguments,
        reason = "INV-004 keeps every typed acceptance obligation explicit"
    )]
    pub(super) const fn new(
        contract_bound: CheckResult,
        observations_fresh: CheckResult,
        gates_complete: CheckResult,
        evidence_complete: CheckResult,
        reviews_complete: CheckResult,
        blockers_complete: CheckResult,
        approvals_complete: CheckResult,
    ) -> Self {
        Self {
            contract_bound,
            observations_fresh,
            gates_complete,
            evidence_complete,
            reviews_complete,
            blockers_complete,
            approvals_complete,
        }
    }
    pub(crate) const fn is_complete(self) -> bool {
        self.contract_bound.is_complete()
            && self.observations_fresh.is_complete()
            && self.gates_complete.is_complete()
            && self.evidence_complete.is_complete()
            && self.reviews_complete.is_complete()
            && self.blockers_complete.is_complete()
            && self.approvals_complete.is_complete()
    }
}
#[doc = " Pure logical acceptance result."]
#[doc = ""]
#[doc = " Construction is private: ordinary callers cannot mint an acceptable result or erase unmet"]
#[doc = " conditions. The evaluator is the only production constructor."]
#[derive(Debug, Eq, PartialEq)]
pub struct AcceptanceDecision {
    unmet: Vec<UnmetCondition>,
    checks: AcceptanceChecks,
    gate_attempt_limit: u16,
    review_cycle_limit: u16,
}
impl AcceptanceDecision {
    pub(super) const fn from_evaluation(
        unmet: Vec<UnmetCondition>,
        checks: AcceptanceChecks,
        gate_attempt_limit: u16,
        review_cycle_limit: u16,
    ) -> Self {
        Self { unmet, checks, gate_attempt_limit, review_cycle_limit }
    }
    #[doc = " Returns `true` exactly when no acceptance condition is unmet."]
    #[must_use]
    pub const fn is_acceptable(&self) -> bool {
        self.unmet.is_empty() && self.checks.is_complete()
    }
    #[doc = " Returns all unmet conditions in deterministic evaluator order."]
    #[must_use]
    pub const fn unmet_conditions(&self) -> &[UnmetCondition] {
        self.unmet.as_slice()
    }
    #[doc = " Returns the gate-attempt budget used for this decision."]
    #[must_use]
    pub const fn gate_attempt_limit(&self) -> u16 {
        self.gate_attempt_limit
    }
    #[doc = " Returns the review-cycle budget used for this decision."]
    #[must_use]
    pub const fn review_cycle_limit(&self) -> u16 {
        self.review_cycle_limit
    }
}
