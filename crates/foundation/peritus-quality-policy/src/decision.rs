//! Deterministic acceptance decision and typed unmet conditions.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::GateFailure;
use peritus_spec::{EvidenceRequirementId, FindingSeverity, ReviewCategory};
use peritus_types::{ActorId, FindingId, GateId, ReviewCycleId};
use vstd::prelude::*;

verus! {

/// Kind of exact-revision observation reported as stale.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationKind {
    /// Deterministic gate result.
    Gate,
    /// Reviewer result.
    Review,
    /// Required artifact.
    Evidence,
    /// Human approval result.
    Approval,
    /// Finding waiver.
    Waiver,
}

/// Reviewer-independence rule not met by the current quorum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewerIndependenceFailure {
    /// Two current reviews used the same reviewer actor.
    DistinctReviewers,
    /// At least one reviewer was not independent from the producer.
    ProducerIndependence,
    /// Two reviews used the same fresh-context identity.
    DistinctContexts,
    /// Two reviews used the same model family.
    DistinctModelFamilies,
    /// Two reviews used the same provider.
    DistinctProviders,
    /// Two reviews declared the same causal ancestry.
    SharedAncestry,
}

/// Reason a supplied waiver cannot resolve its finding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvalidWaiverReason {
    /// The contract forbids waivers.
    Forbidden,
    /// The finding was not explicitly dispositioned as waiver-requested.
    NotRequested,
    /// The waiver named an authority other than the contract authority.
    WrongAuthority,
    /// The waiver named the wrong evidence declaration.
    WrongEvidenceRequirement,
    /// The waiver lacks a current matching approved human authority observation.
    MissingApproval,
    /// The matching human authority explicitly denied the waiver.
    ApprovalDenied,
    /// The waiver references a finding absent from current reviews.
    UnknownFinding,
    /// The referenced waiver evidence artifact is absent or stale.
    MissingEvidence,
    /// A waiver was supplied for a finding that was already resolved.
    AlreadyResolved,
}

/// One precise reason the current revision is not acceptable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnmetCondition {
    /// The requested tuple does not name the contract's exact acceptance-specification identity.
    ContractRevisionMismatch,
    /// An observation belongs to another revision tuple.
    StaleObservation {
        /// Observation collection.
        kind: ObservationKind,
        /// Canonical zero-based position in that collection.
        index: usize,
    },
    /// A gate observation names no gate in the contract.
    UnknownGate(GateId),
    /// A required gate has no current observation.
    MissingGate(GateId),
    /// A required current gate did not pass.
    GateDidNotPass {
        /// Required gate.
        gate_id: GateId,
        /// Normalized failure reason.
        failure: GateFailure,
    },
    /// A passing gate observation was produced after the contract's attempt budget.
    GateAttemptLimitExceeded {
        /// Required gate.
        gate_id: GateId,
        /// One-based observed attempt.
        attempt: u16,
        /// Contract maximum.
        maximum: u16,
    },
    /// An evidence observation names no contract requirement.
    UnknownEvidence(EvidenceRequirementId),
    /// A contract evidence requirement has no current artifact.
    MissingEvidence(EvidenceRequirementId),
    /// A review claims a category not declared by the contract.
    UnknownReviewCategory(ReviewCategory),
    /// No current review covers the required category.
    MissingReviewCategory(ReviewCategory),
    /// Too few current reviews are present.
    ReviewerQuorum {
        /// Contract-required review count.
        required: u16,
        /// Number of current reviews, saturated to `u16::MAX`.
        observed: u16,
    },
    /// A current review was produced after the contract's review-cycle budget.
    ReviewCycleLimitExceeded {
        /// Stable review-cycle identity.
        cycle_id: ReviewCycleId,
        /// One-based observed cycle ordinal.
        cycle: u16,
        /// Contract maximum.
        maximum: u16,
    },
    /// Current reviews violate a configured independence constraint.
    ReviewerIndependence(ReviewerIndependenceFailure),
    /// A finding at or above the blocker threshold remains unresolved and unwaived.
    UnwaivedBlocker {
        /// Stable finding identity.
        finding_id: FindingId,
        /// Normalized severity.
        severity: FindingSeverity,
    },
    /// A supplied waiver is unusable.
    InvalidWaiver {
        /// Finding targeted by the waiver.
        finding_id: FindingId,
        /// Exact reason the waiver failed.
        reason: InvalidWaiverReason,
    },
    /// Final human acceptance approval is required but absent.
    MissingHumanApproval,
    /// A supplied final human approval named the wrong authority policy.
    WrongHumanApprovalAuthority,
    /// Human authority explicitly denied final acceptance.
    HumanApprovalDenied,
    /// An approval observation is unrelated to a configured acceptance or supplied waiver.
    UnexpectedApproval(ActorId),
}

/// Fail-closed status of one independently computed acceptance obligation.
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
    pub(super) const fn from_bool(value: bool) -> (result: Self)
        ensures result.spec_is_complete() == value
    {
        if value { Self::Complete } else { Self::Incomplete }
    }

    pub(super) open spec fn spec_is_complete(self) -> bool { self == Self::Complete }

    pub(super) const fn is_complete(self) -> (complete: bool)
        ensures complete == self.spec_is_complete()
    {
        matches!(self, Self::Complete)
    }
}

/// Independently computed completeness facts carried by every evaluator result.
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
    #[allow(clippy::too_many_arguments, reason = "INV-004 keeps every typed acceptance obligation explicit")]
    pub(super) const fn new(
        contract_bound: CheckResult,
        observations_fresh: CheckResult,
        gates_complete: CheckResult,
        evidence_complete: CheckResult,
        reviews_complete: CheckResult,
        blockers_complete: CheckResult,
        approvals_complete: CheckResult,
    ) -> (result: Self)
        ensures result.spec_complete() == crate::model::acceptance_complete(
            contract_bound.spec_is_complete(),
            observations_fresh.spec_is_complete(),
            gates_complete.spec_is_complete(),
            evidence_complete.spec_is_complete(),
            reviews_complete.spec_is_complete(),
            blockers_complete.spec_is_complete(),
            approvals_complete.spec_is_complete(),
        )
    {
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

    pub(crate) closed spec fn spec_complete(&self) -> bool {
        crate::model::acceptance_complete(
            self.contract_bound.spec_is_complete(),
            self.observations_fresh.spec_is_complete(),
            self.gates_complete.spec_is_complete(),
            self.evidence_complete.spec_is_complete(),
            self.reviews_complete.spec_is_complete(),
            self.blockers_complete.spec_is_complete(),
            self.approvals_complete.spec_is_complete(),
        )
    }

    pub(crate) const fn is_complete(self) -> (complete: bool)
        ensures complete == self.spec_complete()
    {
        self.contract_bound.is_complete()
            && self.observations_fresh.is_complete()
            && self.gates_complete.is_complete()
            && self.evidence_complete.is_complete()
            && self.reviews_complete.is_complete()
            && self.blockers_complete.is_complete()
            && self.approvals_complete.is_complete()
    }
}

/// Pure logical acceptance result.
///
/// Construction is private: ordinary callers cannot mint an acceptable result or erase unmet
/// conditions. The evaluator is the only production constructor.
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
    ) -> (decision: Self)
        ensures
            decision.spec_is_acceptable() ==> checks.spec_complete(),
            decision.spec_is_acceptable() ==> decision.spec_unmet_conditions().len() == 0,
            decision.spec_unmet_conditions() == unmet@,
            decision.spec_gate_attempt_limit() == gate_attempt_limit,
            decision.spec_review_cycle_limit() == review_cycle_limit,
    {
        reveal(AcceptanceDecision::spec_is_acceptable);
        Self { unmet, checks, gate_attempt_limit, review_cycle_limit }
    }

    /// Returns `true` exactly when no acceptance condition is unmet.
    #[must_use]
    pub const fn is_acceptable(&self) -> (acceptable: bool)
        ensures acceptable == self.spec_is_acceptable()
    {
        self.unmet.is_empty() && self.checks.is_complete()
    }

    /// Returns all unmet conditions in deterministic evaluator order.
    #[must_use]
    pub const fn unmet_conditions(&self) -> (conditions: &[UnmetCondition])
        ensures conditions@ == self.spec_unmet_conditions()
    { self.unmet.as_slice() }

    /// Returns the gate-attempt budget used for this decision.
    #[must_use]
    pub const fn gate_attempt_limit(&self) -> (limit: u16)
        ensures limit == self.spec_gate_attempt_limit()
    { self.gate_attempt_limit }

    /// Returns the review-cycle budget used for this decision.
    #[must_use]
    pub const fn review_cycle_limit(&self) -> (limit: u16)
        ensures limit == self.spec_review_cycle_limit()
    { self.review_cycle_limit }

    /// Specification view of the typed unmet conditions.
    pub closed spec fn spec_unmet_conditions(&self) -> Seq<UnmetCondition> { self.unmet@ }

    /// Specification view of the applied gate-attempt budget.
    pub closed spec fn spec_gate_attempt_limit(&self) -> u16 { self.gate_attempt_limit }

    /// Specification view of the applied review-cycle budget.
    pub closed spec fn spec_review_cycle_limit(&self) -> u16 { self.review_cycle_limit }

    /// Specification view of whether this decision is acceptable.
    pub closed spec fn spec_is_acceptable(&self) -> bool {
        self.unmet@.len() == 0 && self.checks.spec_complete()
    }

    /// Specification view of whether every evaluator phase reported complete.
    pub closed spec fn spec_checks_complete(&self) -> bool {
        self.checks.spec_complete()
    }

    pub(crate) proof fn accepted_has_complete_checks(&self)
        requires self.spec_is_acceptable(),
        ensures self.spec_checks_complete(),
    {
        reveal(AcceptanceDecision::spec_is_acceptable);
        reveal(AcceptanceDecision::spec_checks_complete);
    }

    pub(crate) proof fn accepted_has_no_unmet_conditions(&self)
        requires self.spec_is_acceptable(),
        ensures self.spec_unmet_conditions().len() == 0,
    {
        reveal(AcceptanceDecision::spec_is_acceptable);
        reveal(AcceptanceDecision::spec_unmet_conditions);
    }
}

} // verus!
